use super::{
    AssemblyError, Gpr, gpr, number, operand_count, reg_index, signed_field, sized_field,
    split_operands, unsigned,
};
use crate::formats::VirtualAddress;

#[derive(Clone, Copy, PartialEq)]
enum IndexMode {
    Offset,
    Pre,
    Post,
}

enum Offset {
    Immediate(i128, IndexMode),
    Register {
        index: u32,
        option: u32,
        scaled: bool,
    },
}

#[derive(Clone, Copy, PartialEq)]
enum Class {
    W,
    X,
    S,
    D,
    Q,
}

impl Class {
    const fn general(self) -> bool {
        matches!(self, Self::W | Self::X)
    }

    const fn shift(self) -> u32 {
        match self {
            Self::W | Self::S => 2,
            Self::X | Self::D => 3,
            Self::Q => 4,
        }
    }
}

#[derive(Clone, Copy)]
struct Transfer {
    index: u32,
    class: Class,
}

pub(super) fn encode(
    mnemonic: &str,
    operands: &[&str],
    resolve: &mut impl FnMut(&str) -> Option<VirtualAddress>,
) -> Result<u32, AssemblyError> {
    if matches!(mnemonic, "ldp" | "stp") {
        return pair(mnemonic, operands, resolve);
    }
    let ([rt, memory] | [rt, memory, _]) = operands else {
        return operand_count();
    };
    let rt = register(rt)?;
    let load = mnemonic.starts_with('l');
    let unscaled = mnemonic.starts_with("ldur") || mnemonic.starts_with("stur");
    let (opcode, shift) = if mnemonic.ends_with("sw") {
        if rt.class != Class::X {
            return Err(AssemblyError::UnsupportedForm(
                "LDRSW requires an X register",
            ));
        }
        (0xb980_0000, 2)
    } else if mnemonic.ends_with('b') || mnemonic.ends_with('h') {
        if rt.class != Class::W {
            return Err(AssemblyError::UnsupportedForm(
                "Byte and halfword transfers require a W register",
            ));
        }
        let half = mnemonic.ends_with('h');
        (
            0x3900_0000 | (u32::from(half) << 30) | (u32::from(load) << 22),
            u32::from(half),
        )
    } else {
        let base = match rt.class {
            Class::W => 0xb900_0000,
            Class::X => 0xf900_0000,
            Class::S => 0xbd00_0000,
            Class::D => 0xfd00_0000,
            Class::Q => 0x3d80_0000,
        };
        (base | (u32::from(load) << 22), rt.class.shift())
    };
    let (base, offset) = address(memory, operands.get(2).copied(), shift, resolve)?;
    let registers = (base.index << 5) | rt.index;
    let unscaled_opcode = opcode & !0x0100_0000;
    match offset {
        Offset::Register {
            index,
            option,
            scaled,
        } if !unscaled => Ok(unscaled_opcode
            | 0x0020_0800
            | (index << 16)
            | (option << 13)
            | (u32::from(scaled) << 12)
            | registers),
        Offset::Register { .. } => Err(AssemblyError::UnsupportedForm(
            "Unscaled memory operands require a signed immediate",
        )),
        Offset::Immediate(displacement, mode) => {
            if unscaled && mode != IndexMode::Offset {
                return Err(AssemblyError::UnsupportedForm(
                    "Unscaled mnemonics do not allow writeback",
                ));
            }
            check_writeback(mode, base, rt)?;
            let scale = 1_i128 << shift;
            if mode == IndexMode::Offset
                && !unscaled
                && displacement % scale == 0
                && (0..4096).contains(&(displacement / scale))
            {
                return Ok(opcode | ((displacement / scale) as u32) << 10 | registers);
            }
            let field = signed_field(displacement, 9)?;
            let mode = match mode {
                IndexMode::Offset => 0,
                IndexMode::Pre => 3,
                IndexMode::Post => 1,
            };
            Ok(unscaled_opcode | (field << 12) | (mode << 10) | registers)
        }
    }
}

fn pair(
    mnemonic: &str,
    operands: &[&str],
    resolve: &mut impl FnMut(&str) -> Option<VirtualAddress>,
) -> Result<u32, AssemblyError> {
    let ([first, second, memory] | [first, second, memory, _]) = operands else {
        return operand_count();
    };
    let first = register(first)?;
    let second = register(second)?;
    if first.class != second.class || (mnemonic == "ldp" && first.index == second.index) {
        return Err(AssemblyError::UnsupportedForm(
            "Register pair requires matching widths and distinct load destinations",
        ));
    }
    let shift = first.class.shift();
    let (base, offset) = address(memory, operands.get(3).copied(), shift, resolve)?;
    let Offset::Immediate(displacement, mode) = offset else {
        return Err(AssemblyError::UnsupportedForm(
            "Register pairs require an immediate offset",
        ));
    };
    check_writeback(mode, base, first)?;
    check_writeback(mode, base, second)?;
    let field = sized_field(displacement, 7, 1 << shift)?;
    let opcode = match first.class {
        Class::W => 0x2900_0000,
        Class::X => 0xa900_0000,
        Class::S => 0x2d00_0000,
        Class::D => 0x6d00_0000,
        Class::Q => 0xad00_0000,
    };
    let opcode = match mode {
        IndexMode::Offset => opcode,
        IndexMode::Pre => opcode | 0x0080_0000,
        IndexMode::Post => (opcode & !0x0100_0000) | 0x0080_0000,
    };
    Ok(opcode
        | (u32::from(mnemonic == "ldp") << 22)
        | (field << 15)
        | (second.index << 10)
        | (base.index << 5)
        | first.index)
}

fn address(
    source: &str,
    post: Option<&str>,
    shift: u32,
    resolve: &mut impl FnMut(&str) -> Option<VirtualAddress>,
) -> Result<(Gpr, Offset), AssemblyError> {
    let pre = source.ends_with('!');
    let source = source.strip_suffix('!').unwrap_or(source).trim_end();
    let inner = source
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .ok_or(AssemblyError::Syntax(
            "Expected an AArch64 bracketed memory operand",
        ))?;
    let parts = split_operands(inner)?;
    let (base, offset) = match (parts.as_slice(), pre, post) {
        ([base], false, Some(post)) => (
            *base,
            Offset::Immediate(number(post, resolve)?, IndexMode::Post),
        ),
        ([base], false, None) => (*base, Offset::Immediate(0, IndexMode::Offset)),
        ([base, displacement], _, None) if displacement.starts_with('#') => (
            *base,
            Offset::Immediate(
                number(displacement, resolve)?,
                if pre {
                    IndexMode::Pre
                } else {
                    IndexMode::Offset
                },
            ),
        ),
        ([base, index], false, None) => (*base, register_offset(index, None, shift, resolve)?),
        ([base, index, extension], false, None) => (
            *base,
            register_offset(index, Some(extension), shift, resolve)?,
        ),
        _ => {
            return Err(AssemblyError::Syntax(
                "Invalid indexed AArch64 memory operand",
            ));
        }
    };
    Ok((gpr(base)?.address()?, offset))
}

fn register_offset(
    index: &str,
    extension: Option<&str>,
    size_shift: u32,
    resolve: &mut impl FnMut(&str) -> Option<VirtualAddress>,
) -> Result<Offset, AssemblyError> {
    let index = gpr(index)?.plain()?;
    let (option, scaled) = if let Some(extension) = extension {
        let (operation, amount) = extension
            .split_once(char::is_whitespace)
            .unwrap_or((extension, ""));
        let option = match (operation.to_ascii_lowercase().as_str(), index.wide) {
            ("uxtw", false) => 2,
            ("lsl", true) => 3,
            ("sxtw", false) => 6,
            ("sxtx", true) => 7,
            _ => {
                return Err(AssemblyError::UnsupportedForm(
                    "Address extension and index width do not match",
                ));
            }
        };
        let amount = amount.trim();
        let scaled = if amount.is_empty() {
            if option == 3 {
                return Err(AssemblyError::Syntax("Expected an LSL address shift"));
            }
            false
        } else {
            if !amount.starts_with('#') {
                return Err(AssemblyError::Syntax("Expected an immediate address shift"));
            }
            let amount = unsigned(amount, 3, resolve)? as u32;
            if amount != 0 && amount != size_shift {
                return Err(AssemblyError::OutOfRange(
                    "Address shift must match the transfer size",
                ));
            }
            amount == size_shift
        };
        (option, scaled)
    } else if index.wide {
        (3, false)
    } else {
        return Err(AssemblyError::UnsupportedForm(
            "A W index requires UXTW or SXTW",
        ));
    };
    Ok(Offset::Register {
        index: index.index,
        option,
        scaled,
    })
}

fn register(text: &str) -> Result<Transfer, AssemblyError> {
    let lower = text.to_ascii_lowercase();
    let class = match lower.as_bytes().first() {
        Some(b's') if lower != "sp" => Some(Class::S),
        Some(b'd') => Some(Class::D),
        Some(b'q') => Some(Class::Q),
        _ => None,
    };
    if let Some(class) = class {
        return Ok(Transfer {
            index: reg_index(&lower[1..], 31)?,
            class,
        });
    }
    let register = gpr(text)?.plain()?;
    Ok(Transfer {
        index: register.index,
        class: if register.wide { Class::X } else { Class::W },
    })
}

fn check_writeback(mode: IndexMode, base: Gpr, reg: Transfer) -> Result<(), AssemblyError> {
    if mode != IndexMode::Offset && !base.stack && reg.class.general() && base.index == reg.index {
        return Err(AssemblyError::UnsupportedForm(
            "Writeback address overlaps a transfer register",
        ));
    }
    Ok(())
}

pub(super) fn atomic(mnemonic: &str, operands: &[&str]) -> Result<u32, AssemblyError> {
    let [source, dest, address] = operands else {
        return operand_count();
    };
    let source = gpr(source)?.plain()?;
    let dest = gpr(dest)?.plain()?;
    source.same_width(dest)?;
    let base = address
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .ok_or(AssemblyError::Syntax(
            "Expected an atomic memory base register",
        ))?;
    let base = gpr(base.trim())?.address()?;
    let acquire = matches!(mnemonic, "ldadda" | "ldaddal");
    let release = matches!(mnemonic, "ldaddl" | "ldaddal");
    Ok(0xb820_0000
        | (u32::from(source.wide) << 30)
        | (u32::from(acquire) << 23)
        | (u32::from(release) << 22)
        | (source.index << 16)
        | (base.index << 5)
        | dest.index)
}

pub(super) fn authenticated_or_tag(
    mnemonic: &str,
    operands: &[&str],
    resolve: &mut impl FnMut(&str) -> Option<VirtualAddress>,
) -> Result<u32, AssemblyError> {
    let ([rt, memory] | [rt, memory, _]) = operands else {
        return operand_count();
    };
    let tag = mnemonic == "stg";
    let transfer = if tag {
        gpr(rt)?.address()?
    } else {
        gpr(rt)?.plain()?
    };
    if !transfer.wide {
        return Err(AssemblyError::UnsupportedForm(
            "Authenticated and tag transfers require an X register",
        ));
    }
    let shift = if tag { 4 } else { 3 };
    let (base, offset) = address(memory, operands.get(2).copied(), shift, resolve)?;
    let Offset::Immediate(displacement, mode) = offset else {
        return Err(AssemblyError::UnsupportedForm(
            "This transfer requires an immediate offset",
        ));
    };
    let rt = Transfer {
        index: transfer.index,
        class: Class::X,
    };
    check_writeback(mode, base, rt)?;
    let regs = (base.index << 5) | rt.index;
    if tag {
        let field = sized_field(displacement, 9, 16)?;
        let mode = match mode {
            IndexMode::Offset => 2,
            IndexMode::Pre => 3,
            IndexMode::Post => 1,
        };
        return Ok(0xd920_0000 | (field << 12) | (mode << 10) | regs);
    }
    if mode == IndexMode::Post {
        return Err(AssemblyError::UnsupportedForm(
            "Authenticated load supports offset or pre-index addressing",
        ));
    }
    let field = sized_field(displacement, 10, 8)?;
    Ok(0xf820_0400
        | (u32::from(mnemonic == "ldrab") << 23)
        | ((field & 0x200) << 13)
        | ((field & 0x1ff) << 12)
        | (u32::from(mode == IndexMode::Pre) << 11)
        | regs)
}
