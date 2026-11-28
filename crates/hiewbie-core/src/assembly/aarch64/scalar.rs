use super::{AssemblyError, Gpr, gpr, number, operand_count, unsigned};
use crate::formats::VirtualAddress;

pub(super) fn encode(
    mnemonic: &str,
    operands: &[&str],
    resolve: &mut impl FnMut(&str) -> Option<VirtualAddress>,
) -> Result<u32, AssemblyError> {
    match mnemonic {
        "add" | "adds" | "sub" | "subs" | "cmp" | "cmn" => add_sub(mnemonic, operands, resolve),
        "mov" | "movn" | "movz" | "movk" => move_value(mnemonic, operands, resolve),
        "and" | "ands" | "orr" | "eor" | "tst" => logical(mnemonic, operands, resolve),
        "lsl" | "lsr" | "asr" | "ror" => shift(mnemonic, operands, resolve),
        "mul" | "madd" | "msub" | "sdiv" | "udiv" => multiply_divide(mnemonic, operands),
        "brk" | "svc" => {
            let [imm] = operands else {
                return operand_count();
            };
            let base = if mnemonic == "brk" {
                0xd420_0000
            } else {
                0xd400_0001
            };
            Ok(base | ((unsigned(imm, 16, resolve)? as u32) << 5))
        }
        "br" | "blr" | "ret" => {
            let rn = match operands {
                [rn] => xreg(rn)?,
                [] if mnemonic == "ret" => 30,
                _ => return operand_count(),
            };
            let base = match mnemonic {
                "br" => 0xd61f_0000,
                "blr" => 0xd63f_0000,
                _ => 0xd65f_0000,
            };
            Ok(base | (rn << 5))
        }
        "braa" | "brab" | "blraa" | "blrab" => {
            let [rn, rm] = operands else {
                return operand_count();
            };
            let link = mnemonic.starts_with("blr");
            let key_b = mnemonic.ends_with('b');
            Ok(0xd71f_0800
                | (u32::from(link) << 21)
                | (u32::from(key_b) << 10)
                | (xreg(rn)? << 5)
                | gpr(rm)?.address()?.index)
        }
        "bti" => {
            let operation = match operands {
                [] => 0,
                [kind] => match kind.to_ascii_lowercase().as_str() {
                    "c" => 1,
                    "j" => 2,
                    "jc" => 3,
                    _ => return Err(AssemblyError::Syntax("Expected BTI C, J, or JC")),
                },
                _ => return operand_count(),
            };
            Ok(0xd503_241f | (operation << 6))
        }
        "smstart" | "smstop" => {
            let selection = match operands {
                [] => 3,
                [kind] => match kind.to_ascii_lowercase().as_str() {
                    "sm" => 1,
                    "za" => 2,
                    _ => return Err(AssemblyError::Syntax("Expected an SME SM or ZA selector")),
                },
                _ => return operand_count(),
            };
            Ok(0xd503_407f | (selection << 9) | (u32::from(mnemonic == "smstart") << 8))
        }
        "crc32x" | "crc32cx" => {
            let [rd, rn, rm] = operands else {
                return operand_count();
            };
            let rd = gpr(rd)?.plain()?;
            let rn = gpr(rn)?.plain()?;
            if rd.wide || rn.wide {
                return Err(AssemblyError::UnsupportedForm(
                    "CRC32X uses W for the result and accumulator",
                ));
            }
            let base = if mnemonic == "crc32cx" {
                0x9ac0_5c00
            } else {
                0x9ac0_4c00
            };
            Ok(base | (xreg(rm)? << 16) | (rn.index << 5) | rd.index)
        }
        "nop" | "paciasp" | "pacibsp" | "autiasp" | "autibsp" | "retaa" | "retab" => {
            if !operands.is_empty() {
                return operand_count();
            }
            Ok(match mnemonic {
                "nop" => 0xd503_201f,
                "paciasp" => 0xd503_233f,
                "pacibsp" => 0xd503_237f,
                "autiasp" => 0xd503_23bf,
                "autibsp" => 0xd503_23ff,
                "retaa" => 0xd65f_0bff,
                "retab" => 0xd65f_0fff,
                _ => unreachable!(),
            })
        }
        _ => Err(AssemblyError::UnknownMnemonic(mnemonic.into())),
    }
}

fn xreg(text: &str) -> Result<u32, AssemblyError> {
    let register = gpr(text)?.plain()?;
    if register.wide {
        Ok(register.index)
    } else {
        Err(AssemblyError::UnsupportedForm("Expected an X register"))
    }
}

fn add_sub(
    mnemonic: &str,
    operands: &[&str],
    resolve: &mut impl FnMut(&str) -> Option<VirtualAddress>,
) -> Result<u32, AssemblyError> {
    let compare = matches!(mnemonic, "cmp" | "cmn");
    let len = if compare { 2 } else { 3 };
    if operands.len() != len && operands.len() != len + 1 {
        return operand_count();
    }
    let rn = gpr(operands[len - 2])?;
    let rd = if compare {
        Gpr {
            index: 31,
            wide: rn.wide,
            stack: false,
        }
    } else {
        gpr(operands[0])?
    };
    rd.same_width(rn)?;
    let right = operands[len - 1];
    let modifier = operands.get(len).copied();
    let flags = compare || mnemonic.ends_with('s');
    let subtract = matches!(mnemonic, "cmp" | "sub" | "subs");
    let common = rd.sf()
        | (u32::from(subtract) << 30)
        | (u32::from(flags) << 29)
        | (rn.index << 5)
        | rd.index;
    if right.starts_with('#') {
        check_add_sub_sp(rd, rn, flags)?;
        let (imm, shifted) = if let Some(modifier) = modifier {
            let (name, shift) = modifier_parts(modifier, resolve)?;
            let shifted = name == "lsl" && shift == 12;
            if !shifted && (name != "lsl" || shift != 0) {
                return Err(AssemblyError::OutOfRange(
                    "ADD/SUB immediate shift must be LSL 0 or 12",
                ));
            }
            (unsigned(right, 12, resolve)? as u32, shifted)
        } else {
            let value = unsigned(right, 24, resolve)? as u32;
            if value < 4096 {
                (value, false)
            } else if value & 0xfff == 0 {
                (value >> 12, true)
            } else {
                return Err(AssemblyError::OutOfRange(
                    "ADD/SUB immediate does not fit one instruction",
                ));
            }
        };
        return Ok(0x1100_0000 | common | (u32::from(shifted) << 22) | (imm << 10));
    }
    let rm = gpr(right)?.plain()?;
    let (modifier, shift) = if let Some(modifier) = modifier {
        modifier_parts(modifier, resolve)?
    } else {
        ("lsl".to_owned(), 0)
    };
    let is_extend = matches!(
        modifier.as_str(),
        "uxtb" | "uxth" | "uxtw" | "uxtx" | "sxtb" | "sxth" | "sxtw" | "sxtx"
    );
    if is_extend || rd.stack || rn.stack {
        check_add_sub_sp(rd, rn, flags)?;
        let option = match modifier.as_str() {
            "uxtb" => 0,
            "uxth" => 1,
            "uxtw" => 2,
            "uxtx" => 3,
            "sxtb" => 4,
            "sxth" => 5,
            "sxtw" => 6,
            "sxtx" => 7,
            "lsl" => {
                if rn.wide {
                    3
                } else {
                    2
                }
            }
            _ => {
                return Err(AssemblyError::UnsupportedForm(
                    "Expected an ADD/SUB register extension",
                ));
            }
        };
        if shift > 4 || rm.wide != matches!(option, 3 | 7) || (rm.wide && !rn.wide) {
            return Err(AssemblyError::UnsupportedForm(
                "ADD/SUB register extension or shift does not match the widths",
            ));
        }
        return Ok(0x0b20_0000 | common | (rm.index << 16) | (option << 13) | (shift << 10));
    }
    rd.same_width(rm)?;
    let field = shift_field(&modifier, shift, rn.width(), false)?;
    Ok(0x0b00_0000 | common | (rm.index << 16) | field)
}

fn check_add_sub_sp(rd: Gpr, rn: Gpr, flags: bool) -> Result<(), AssemblyError> {
    if (rd.index == 31 && rd.stack == flags) || (rn.index == 31 && !rn.stack) {
        return Err(AssemblyError::UnsupportedForm(
            "SP or zero register is invalid for this ADD/SUB form",
        ));
    }
    Ok(())
}

fn move_value(
    mnemonic: &str,
    operands: &[&str],
    resolve: &mut impl FnMut(&str) -> Option<VirtualAddress>,
) -> Result<u32, AssemblyError> {
    let ([dest, source] | [dest, source, _]) = operands else {
        return operand_count();
    };
    let dest = gpr(dest)?;
    if mnemonic == "mov" {
        if operands.len() != 2 {
            return operand_count();
        }
        if source.starts_with('#') {
            let imm = width_value(source, dest.width(), resolve)?;
            return move_immediate(dest, imm);
        }
        let source = gpr(source)?;
        dest.same_width(source)?;
        return if dest.stack || source.stack {
            if (dest.index == 31 && !dest.stack) || (source.index == 31 && !source.stack) {
                return Err(AssemblyError::UnsupportedForm(
                    "MOV cannot combine SP and a zero register",
                ));
            }
            Ok(0x1100_0000 | dest.sf() | (source.index << 5) | dest.index)
        } else {
            Ok(0x2a00_03e0 | dest.sf() | (source.index << 16) | dest.index)
        };
    }
    dest.plain()?;
    let imm = unsigned(source, 16, resolve)? as u32;
    let shift = if let Some(modifier) = operands.get(2) {
        let (name, shift) = modifier_parts(modifier, resolve)?;
        if name != "lsl" || shift >= dest.width() || !shift.is_multiple_of(16) {
            return Err(AssemblyError::OutOfRange(
                "MOV wide shift must be a multiple of 16 within the destination width",
            ));
        }
        shift / 16
    } else {
        0
    };
    let base = match mnemonic {
        "movn" => 0x1280_0000,
        "movz" => 0x5280_0000,
        _ => 0x7280_0000,
    };
    Ok(base | dest.sf() | (shift << 21) | (imm << 5) | dest.index)
}

fn logical(
    mnemonic: &str,
    operands: &[&str],
    resolve: &mut impl FnMut(&str) -> Option<VirtualAddress>,
) -> Result<u32, AssemblyError> {
    let test = mnemonic == "tst";
    let len = if test { 2 } else { 3 };
    if operands.len() != len && operands.len() != len + 1 {
        return operand_count();
    }
    let rn = gpr(operands[len - 2])?.plain()?;
    let rd = if test {
        Gpr {
            index: 31,
            wide: rn.wide,
            stack: false,
        }
    } else {
        gpr(operands[0])?
    };
    rd.same_width(rn)?;
    let op = match mnemonic {
        "and" => 0,
        "orr" => 1,
        "eor" => 2,
        _ => 3,
    };
    let common = rd.sf() | (op << 29) | (rn.index << 5) | rd.index;
    let right = operands[len - 1];
    if right.starts_with('#') {
        if operands.len() != len {
            return operand_count();
        }
        if rd.index == 31 && rd.stack == (op == 3) {
            return Err(AssemblyError::UnsupportedForm(
                "SP or zero register is invalid for this logical immediate destination",
            ));
        }
        let value = width_value(right, rn.width(), resolve)?;
        return Ok(0x1200_0000 | common | logical_mask(value, rn.width())?);
    }
    rd.plain()?;
    let rm = gpr(right)?.plain()?;
    rn.same_width(rm)?;
    let field = if let Some(modifier) = operands.get(len) {
        let (operation, amount) = modifier_parts(modifier, resolve)?;
        shift_field(&operation, amount, rn.width(), true)?
    } else {
        0
    };
    Ok(0x0a00_0000 | common | (rm.index << 16) | field)
}

fn move_immediate(dest: Gpr, value: u64) -> Result<u32, AssemblyError> {
    let registers = dest.sf() | dest.index;
    if !dest.stack {
        for (fill, base) in [(0, 0x5280_0000), (width_mask(dest.width()), 0x1280_0000)] {
            let halfwords = value ^ fill;
            let shift = if halfwords == 0 {
                0
            } else {
                halfwords.trailing_zeros() / 16
            };
            let immediate = halfwords >> (shift * 16);
            if immediate <= u16::MAX.into() {
                return Ok(base | registers | (shift << 21) | ((immediate as u32) << 5));
            }
        }
    }
    if dest.index == 31 && !dest.stack {
        return Err(AssemblyError::UnsupportedForm(
            "MOV bitmask alias cannot target a zero register",
        ));
    }
    Ok(0x3200_03e0 | registers | logical_mask(value, dest.width())?)
}

fn width_mask(width: u32) -> u64 {
    u64::MAX >> (64 - width)
}

fn logical_mask(value: u64, width: u32) -> Result<u32, AssemblyError> {
    let mut size = width;
    let mut element = value;
    while size > 2 {
        let half = size / 2;
        let low = element & width_mask(half);
        if element >> half != low {
            break;
        }
        size = half;
        element = low;
    }

    // A logical immediate is a rotated run of ones in the smallest repeated element.
    let ones = element.count_ones();
    if ones > 0 && ones < size {
        let run = width_mask(ones);
        for rotation in 0..size {
            if element
                == ((run >> rotation) | (run << ((size - rotation) % size))) & width_mask(size)
            {
                let n = u32::from(size == 64);
                let imms = ((0_u32.wrapping_sub(2 * size)) | (ones - 1)) & 0x3f;
                return Ok((n << 22) | (rotation << 16) | (imms << 10));
            }
        }
    }
    Err(AssemblyError::OutOfRange(
        "Immediate is not an AArch64 logical bitmask",
    ))
}

fn width_value(
    text: &str,
    bits: u32,
    resolve: &mut impl FnMut(&str) -> Option<VirtualAddress>,
) -> Result<u64, AssemblyError> {
    let value = number(text, resolve)?;
    if !(-(1_i128 << (bits - 1))..1_i128 << bits).contains(&value) {
        return Err(AssemblyError::OutOfRange(
            "Immediate exceeds the selected register width",
        ));
    }
    Ok((value & ((1_i128 << bits) - 1)) as u64)
}

fn shift(
    mnemonic: &str,
    operands: &[&str],
    resolve: &mut impl FnMut(&str) -> Option<VirtualAddress>,
) -> Result<u32, AssemblyError> {
    let [rd, rn, amount] = operands else {
        return operand_count();
    };
    let rd = gpr(rd)?.plain()?;
    let rn = gpr(rn)?.plain()?;
    rd.same_width(rn)?;
    let regs = rd.sf() | (rn.index << 5) | rd.index;
    if !amount.starts_with('#') {
        let rm = gpr(amount)?.plain()?;
        rd.same_width(rm)?;
        let op = match mnemonic {
            "lsl" => 0,
            "lsr" => 1,
            "asr" => 2,
            _ => 3,
        };
        return Ok(0x1ac0_2000 | regs | (rm.index << 16) | (op << 10));
    }
    let amount = unsigned(amount, 6, resolve)? as u32;
    let width = rd.width();
    if amount >= width {
        return Err(AssemblyError::OutOfRange(
            "Shift exceeds the selected register width",
        ));
    }
    let n = u32::from(rd.wide) << 22;
    if mnemonic == "ror" {
        return Ok(0x1380_0000 | regs | n | (rn.index << 16) | (amount << 10));
    }
    let (immr, imms) = if mnemonic == "lsl" {
        ((width - amount) % width, width - 1 - amount)
    } else {
        (amount, width - 1)
    };
    let base = if mnemonic == "asr" {
        0x1300_0000
    } else {
        0x5300_0000
    };
    Ok(base | regs | n | (immr << 16) | (imms << 10))
}

fn multiply_divide(mnemonic: &str, operands: &[&str]) -> Result<u32, AssemblyError> {
    let ([rd, rn, rm] | [rd, rn, rm, _]) = operands else {
        return operand_count();
    };
    if (operands.len() == 4) != matches!(mnemonic, "madd" | "msub") {
        return operand_count();
    }
    let rd = gpr(rd)?.plain()?;
    let rn = gpr(rn)?.plain()?;
    let rm = gpr(rm)?.plain()?;
    rd.same_width(rn)?;
    rd.same_width(rm)?;
    let fields = rd.sf() | (rm.index << 16) | (rn.index << 5) | rd.index;
    if mnemonic == "sdiv" || mnemonic == "udiv" {
        let sign = u32::from(mnemonic == "sdiv") << 10;
        return Ok(0x1ac0_0800 | fields | sign);
    }
    let addend = if let Some(addend) = operands.get(3) {
        let ra = gpr(addend)?.plain()?;
        rd.same_width(ra)?;
        ra.index
    } else {
        31
    };
    Ok(0x1b00_0000 | fields | (u32::from(mnemonic == "msub") << 15) | (addend << 10))
}

fn modifier_parts(
    modifier: &str,
    resolve: &mut impl FnMut(&str) -> Option<VirtualAddress>,
) -> Result<(String, u32), AssemblyError> {
    let (operation, value) = modifier
        .split_once(char::is_whitespace)
        .unwrap_or((modifier, ""));
    let value = value.trim();
    let shift = if value.is_empty()
        && !matches!(
            operation.to_ascii_lowercase().as_str(),
            "lsl" | "lsr" | "asr" | "ror"
        ) {
        0
    } else if value.starts_with('#') {
        unsigned(value, 6, resolve)? as u32
    } else {
        return Err(AssemblyError::Syntax(
            "Expected an immediate register shift",
        ));
    };
    Ok((operation.to_ascii_lowercase(), shift))
}

fn shift_field(
    operation: &str,
    amount: u32,
    width: u32,
    allow_ror: bool,
) -> Result<u32, AssemblyError> {
    let operation = match operation {
        "lsl" => 0,
        "lsr" => 1,
        "asr" => 2,
        "ror" if allow_ror => 3,
        _ => {
            return Err(AssemblyError::UnsupportedForm(
                "Unsupported shift for this register form",
            ));
        }
    };
    if amount >= width {
        return Err(AssemblyError::OutOfRange(
            "Shift exceeds the selected register width",
        ));
    }
    Ok((operation << 22) | (amount << 10))
}

pub(super) fn system(mnemonic: &str, operands: &[&str]) -> Result<u32, AssemblyError> {
    match (mnemonic, operands) {
        ("dmb" | "dsb" | "isb", [option]) => {
            let option = match option.to_ascii_lowercase().as_str() {
                "oshld" => 1,
                "oshst" => 2,
                "osh" => 3,
                "nshld" => 5,
                "nshst" => 6,
                "nsh" => 7,
                "ishld" => 9,
                "ishst" => 10,
                "ish" => 11,
                "ld" => 13,
                "st" => 14,
                "sy" => 15,
                _ => return Err(AssemblyError::Syntax("Unknown AArch64 barrier option")),
            };
            let base = match mnemonic {
                "dmb" => 0xd503_30bf,
                "dsb" => 0xd503_309f,
                "isb" if option == 15 => 0xd503_30df,
                _ => return Err(AssemblyError::UnsupportedForm("ISB requires SY")),
            };
            Ok(base | (option << 8))
        }
        ("isb", []) => Ok(0xd503_3fdf),
        ("mrs" | "msr", [left, right]) => {
            let read = mnemonic == "mrs";
            let (system, transfer) = if read { (right, left) } else { (left, right) };
            let (opcode, writable) = match system.to_ascii_lowercase().as_str() {
                "tpidr_el0" => (0xd53b_d040, true),
                "tpidrro_el0" => (0xd53b_d060, false),
                "cntfrq_el0" => (0xd53b_e000, false),
                "cntvct_el0" => (0xd53b_e040, false),
                "fpcr" => (0xd53b_4400, true),
                "fpsr" => (0xd53b_4420, true),
                "nzcv" => (0xd53b_4200, true),
                _ => {
                    return Err(AssemblyError::UnsupportedForm(
                        "Unqualified AArch64 system register",
                    ));
                }
            };
            if !read && !writable {
                return Err(AssemblyError::UnsupportedForm(
                    "This system register is read-only",
                ));
            }
            Ok((opcode & !(u32::from(!read) << 21)) | xreg(transfer)?)
        }
        ("irg", [rd, rn] | [rd, rn, _]) => {
            let rd = gpr(rd)?.address()?.index;
            let rn = gpr(rn)?.address()?.index;
            let rm = operands.get(2).map_or(Ok(31), |register| xreg(register))?;
            Ok(0x9ac0_1000 | (rm << 16) | (rn << 5) | rd)
        }
        _ => operand_count(),
    }
}
