use super::{AssemblyError, gpr, operand_count, sized_field, unsigned};
use crate::formats::VirtualAddress;

pub(super) fn encode(
    mnemonic: &str,
    operands: &[&str],
    pc: u64,
    resolve: &mut impl FnMut(&str) -> Option<VirtualAddress>,
) -> Result<u32, AssemblyError> {
    match (mnemonic, operands) {
        ("adr" | "adrp", [rd, target]) => {
            let rd = gpr(rd)?.plain()?;
            if !rd.wide {
                return Err(AssemblyError::UnsupportedForm(
                    "ADR and ADRP require an X register",
                ));
            }
            let target = unsigned(target, 64, resolve)?;
            let page = mnemonic == "adrp";
            let (base, scale, pc, target) = if page {
                (0x9000_0000, 4096, pc & !0xfff, target & !0xfff)
            } else {
                (0x1000_0000, 1, pc, target)
            };
            let field = sized_field(displacement(pc, target), 21, scale)?;
            Ok(base | ((field & 3) << 29) | ((field >> 2) << 5) | rd.index)
        }
        ("b" | "bl", [target]) => {
            let field = sized_field(displacement(pc, unsigned(target, 64, resolve)?), 26, 4)?;
            let base = if mnemonic == "bl" {
                0x9400_0000
            } else {
                0x1400_0000
            };
            Ok(base | field)
        }
        ("cbz" | "cbnz", [rt, target]) => {
            let rt = gpr(rt)?.plain()?;
            let field = sized_field(displacement(pc, unsigned(target, 64, resolve)?), 19, 4)?;
            let nz = u32::from(mnemonic == "cbnz") << 24;
            Ok(0x3400_0000 | rt.sf() | nz | (field << 5) | rt.index)
        }
        ("tbz" | "tbnz", [rt, bit, target]) => {
            let rt = gpr(rt)?.plain()?;
            let bit = unsigned(bit, 6, resolve)? as u32;
            if bit >= rt.width() {
                return Err(AssemblyError::OutOfRange(
                    "Bit index exceeds the selected register width",
                ));
            }
            let field = sized_field(displacement(pc, unsigned(target, 64, resolve)?), 14, 4)?;
            let nz = u32::from(mnemonic == "tbnz") << 24;
            Ok(
                0x3600_0000
                    | ((bit & 32) << 26)
                    | nz
                    | ((bit & 31) << 19)
                    | (field << 5)
                    | rt.index,
            )
        }
        ("ldr" | "ldrsw" | "prfm", [rt, target]) => {
            let (opcode, register) = literal_form(mnemonic, rt)?;
            let field = sized_field(displacement(pc, unsigned(target, 64, resolve)?), 19, 4)?;
            Ok(opcode | (field << 5) | register)
        }
        (name, [target]) if name.starts_with("b.") || name.starts_with("bc.") => {
            let (consistent, suffix) = name
                .strip_prefix("bc.")
                .map_or((false, &name[2..]), |suffix| (true, suffix));
            let cond = condition(suffix)
                .ok_or(AssemblyError::Syntax("Unknown AArch64 branch condition"))?;
            let field = sized_field(displacement(pc, unsigned(target, 64, resolve)?), 19, 4)?;
            Ok(0x5400_0000 | (field << 5) | (u32::from(consistent) << 4) | cond)
        }
        _ => operand_count(),
    }
}

fn displacement(pc: u64, target: u64) -> i128 {
    i128::from(target) - i128::from(pc)
}

fn literal_form(mnemonic: &str, rt: &str) -> Result<(u32, u32), AssemblyError> {
    if mnemonic == "prfm" {
        let lower = rt.to_ascii_lowercase();
        let names = [
            (
                0,
                [
                    "pldl1keep",
                    "pldl1strm",
                    "pldl2keep",
                    "pldl2strm",
                    "pldl3keep",
                    "pldl3strm",
                ],
            ),
            (
                8,
                [
                    "plil1keep",
                    "plil1strm",
                    "plil2keep",
                    "plil2strm",
                    "plil3keep",
                    "plil3strm",
                ],
            ),
            (
                16,
                [
                    "pstl1keep",
                    "pstl1strm",
                    "pstl2keep",
                    "pstl2strm",
                    "pstl3keep",
                    "pstl3strm",
                ],
            ),
        ];
        let field = names
            .into_iter()
            .find_map(|(base, names)| {
                names
                    .iter()
                    .position(|name| *name == lower)
                    .map(|index| base + index as u32)
            })
            .ok_or(AssemblyError::UnsupportedForm(
                "Unknown literal prefetch operation",
            ))?;
        return Ok((0xd800_0000, field));
    }
    if mnemonic == "ldrsw" {
        let rt = gpr(rt)?.plain()?;
        if rt.wide {
            return Ok((0x9800_0000, rt.index));
        }
        return Err(AssemblyError::UnsupportedForm(
            "LDRSW requires an X register",
        ));
    }
    let lower = rt.to_ascii_lowercase();
    if let Some(opcode) = match lower.as_bytes().first() {
        Some(b's') => Some(0x1c00_0000),
        Some(b'd') => Some(0x5c00_0000),
        Some(b'q') => Some(0x9c00_0000),
        _ => None,
    } {
        return Ok((opcode, super::reg_index(&lower[1..], 31)?));
    }
    let rt = gpr(rt)?.plain()?;
    Ok((if rt.wide { 0x5800_0000 } else { 0x1800_0000 }, rt.index))
}

fn condition(text: &str) -> Option<u32> {
    match text {
        "eq" => Some(0),
        "ne" => Some(1),
        "hs" | "cs" => Some(2),
        "lo" | "cc" => Some(3),
        "mi" => Some(4),
        "pl" => Some(5),
        "vs" => Some(6),
        "vc" => Some(7),
        "hi" => Some(8),
        "ls" => Some(9),
        "ge" => Some(10),
        "lt" => Some(11),
        "gt" => Some(12),
        "le" => Some(13),
        "al" => Some(14),
        "nv" => Some(15),
        _ => None,
    }
}
