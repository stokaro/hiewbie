use super::{
    AssemblyError, gpr, number, operand_count, reg_index, signed_field, split_operands, unsigned,
};
use crate::formats::VirtualAddress;

pub(super) fn encode(
    mnemonic: &str,
    operands: &[&str],
    resolve: &mut impl FnMut(&str) -> Option<VirtualAddress>,
) -> Result<u32, AssemblyError> {
    match (mnemonic, operands) {
        ("ptrue", [pd] | [pd, _]) => {
            let (pd, lane) = shaped(pd, 'p', 15)?;
            let element = element(lane)?;
            let pattern = operands.get(1).copied().unwrap_or("all");
            let pattern = if pattern.starts_with('#') {
                unsigned(pattern, 5, resolve)? as u32
            } else {
                match pattern.to_ascii_lowercase().as_str() {
                    "pow2" => 0,
                    "vl1" => 1,
                    "vl2" => 2,
                    "vl3" => 3,
                    "vl4" => 4,
                    "vl5" => 5,
                    "vl6" => 6,
                    "vl7" => 7,
                    "vl8" => 8,
                    "vl16" => 9,
                    "vl32" => 10,
                    "vl64" => 11,
                    "vl128" => 12,
                    "vl256" => 13,
                    "mul4" => 29,
                    "mul3" => 30,
                    "all" => 31,
                    _ => return Err(AssemblyError::Syntax("Unknown PTRUE pattern")),
                }
            };
            Ok(0x2518_e000 | (element << 22) | (pattern << 5) | pd)
        }
        ("whilelt" | "whilelo", [pd, rn, rm]) => {
            let (pd, lane) = shaped(pd, 'p', 15)?;
            let rn = gpr(rn)?.plain()?;
            let rm = gpr(rm)?.plain()?;
            rn.same_width(rm)?;
            Ok(0x2520_0400
                | (element(lane)? << 22)
                | (rm.index << 16)
                | (u32::from(rn.wide) << 12)
                | (u32::from(mnemonic == "whilelo") << 11)
                | (rn.index << 5)
                | pd)
        }
        ("add" | "sub", [rd, rn, rm]) => {
            let (rd, count, size, sve) = vector(rd)?;
            let (rn, first_count, first_size, first_sve) = vector(rn)?;
            let (rm, second_count, second_size, second_sve) = vector(rm)?;
            if (count, size, sve) != (first_count, first_size, first_sve)
                || (count, size, sve) != (second_count, second_size, second_sve)
            {
                return Err(AssemblyError::UnsupportedForm(
                    "Vector arithmetic requires matching register arrangements",
                ));
            }
            let sub = u32::from(mnemonic == "sub");
            let base = if sve {
                0x0420_0000 | (sub << 10)
            } else {
                let quad = u32::from(count << size == 16);
                0x0e20_8400 | (quad << 30) | (sub << 29)
            };
            Ok(base | (size << 22) | (rm << 16) | (rn << 5) | rd)
        }
        ("eor3", [rd, first, rm, rk]) => {
            let rd = shaped_as(rd, 'z', "d")?;
            let first = shaped_as(first, 'z', "d")?;
            let rm = shaped_as(rm, 'z', "d")?;
            let rk = shaped_as(rk, 'z', "d")?;
            if rd != first {
                return Err(AssemblyError::UnsupportedForm(
                    "EOR3 destination and first source must be the same Z register",
                ));
            }
            Ok(0x0420_3800 | (rm << 16) | (rk << 5) | rd)
        }
        ("sqxtnb" | "sqxtnt", [rd, rn]) => {
            let (rd, dest) = shaped(rd, 'z', 31)?;
            let (rn, source) = shaped(rn, 'z', 31)?;
            let (dest, source) = (element(dest)?, element(source)?);
            let size = match (dest, source) {
                (0, 1) => 0x0008_0000,
                (1, 2) => 0x0010_0000,
                (2, 3) => 0x0040_0000,
                _ => {
                    return Err(AssemblyError::UnsupportedForm(
                        "SVE2 narrowing requires elements twice the destination width",
                    ));
                }
            };
            let top = u32::from(mnemonic == "sqxtnt") << 10;
            Ok(0x4520_4000 | size | top | (rn << 5) | rd)
        }
        ("ld1b" | "ld1h" | "ld1w" | "ld1d" | "st1b" | "st1h" | "st1w" | "st1d", [zt, pg, m]) => {
            transfer(mnemonic, zt, pg, m, resolve)
        }
        ("fadd" | "fsub" | "fmul" | "fdiv", [rd, rn, rm]) => {
            let (rd, format) = float(rd)?;
            let (rn, first) = float(rn)?;
            let (rm, second) = float(rm)?;
            if format != first || format != second {
                return Err(AssemblyError::UnsupportedForm(
                    "Floating-point arithmetic requires matching scalar registers",
                ));
            }
            let op = match mnemonic {
                "fadd" => 0x2800,
                "fsub" => 0x3800,
                "fmul" => 0x0800,
                _ => 0x1800,
            };
            Ok(0x1e20_0000 | (format << 22) | (rm << 16) | op | (rn << 5) | rd)
        }
        ("aese" | "aesd" | "aesmc" | "aesimc", [rd, rn]) => {
            let rd = shaped_as(rd, 'v', "16b")?;
            let rn = shaped_as(rn, 'v', "16b")?;
            let op = match mnemonic {
                "aese" => 0x4800,
                "aesd" => 0x5800,
                "aesmc" => 0x6800,
                _ => 0x7800,
            };
            Ok(0x4e28_0000 | op | (rn << 5) | rd)
        }
        ("sha256h" | "sha256h2", [rd, rn, rm]) => {
            let rd = standalone(rd, 'q', 31)?;
            let rn = standalone(rn, 'q', 31)?;
            let rm = shaped_as(rm, 'v', "4s")?;
            let op = if mnemonic == "sha256h" {
                0x4000
            } else {
                0x5000
            };
            Ok(0x5e00_0000 | (rm << 16) | op | (rn << 5) | rd)
        }
        _ => operand_count(),
    }
}

fn standalone(text: &str, prefix: char, maximum: u32) -> Result<u32, AssemblyError> {
    let head = text
        .as_bytes()
        .first()
        .map(|byte| byte.to_ascii_lowercase());
    if head != Some(prefix as u8) {
        return Err(AssemblyError::UnsupportedForm(
            "Unexpected AArch64 vector or predicate register class",
        ));
    }
    reg_index(&text[1..], maximum)
}

fn shaped(text: &str, prefix: char, maximum: u32) -> Result<(u32, &str), AssemblyError> {
    let (register, arrangement) = text.split_once('.').ok_or(AssemblyError::Syntax(
        "Expected an AArch64 register arrangement",
    ))?;
    Ok((standalone(register, prefix, maximum)?, arrangement))
}

fn shaped_as(text: &str, prefix: char, required: &str) -> Result<u32, AssemblyError> {
    let (register, size) = shaped(text, prefix, 31)?;
    if size.eq_ignore_ascii_case(required) {
        Ok(register)
    } else {
        Err(AssemblyError::UnsupportedForm(
            "Unexpected AArch64 vector arrangement",
        ))
    }
}

fn element(lane: &str) -> Result<u32, AssemblyError> {
    match lane.to_ascii_lowercase().as_str() {
        "b" => Ok(0),
        "h" => Ok(1),
        "s" => Ok(2),
        "d" => Ok(3),
        _ => Err(AssemblyError::UnsupportedForm(
            "Expected a B, H, S or D element width",
        )),
    }
}

fn vector(text: &str) -> Result<(u32, u32, u32, bool), AssemblyError> {
    if text.starts_with(['z', 'Z']) {
        let (register, lane) = shaped(text, 'z', 31)?;
        return Ok((register, 0, element(lane)?, true));
    }
    let (register, arrangement) = shaped(text, 'v', 31)?;
    let (count, size) = match arrangement.to_ascii_lowercase().as_str() {
        "8b" => (8, 0),
        "16b" => (16, 0),
        "4h" => (4, 1),
        "8h" => (8, 1),
        "2s" => (2, 2),
        "4s" => (4, 2),
        "2d" => (2, 3),
        _ => {
            return Err(AssemblyError::UnsupportedForm(
                "Unsupported Advanced SIMD arrangement",
            ));
        }
    };
    Ok((register, count, size, false))
}

fn float(text: &str) -> Result<(u32, u32), AssemblyError> {
    let class = match text
        .as_bytes()
        .first()
        .map(|byte| byte.to_ascii_lowercase())
    {
        Some(b's') => 0,
        Some(b'd') => 1,
        Some(b'h') => 3,
        _ => {
            return Err(AssemblyError::UnsupportedForm(
                "Expected an H, S or D floating-point register",
            ));
        }
    };
    Ok((reg_index(&text[1..], 31)?, class))
}

fn transfer(
    mnemonic: &str,
    zt: &str,
    pg: &str,
    memory: &str,
    resolve: &mut impl FnMut(&str) -> Option<VirtualAddress>,
) -> Result<u32, AssemblyError> {
    let load = mnemonic.starts_with('l');
    let (lane, kind) = match mnemonic.as_bytes()[3] {
        b'b' => ("b", 0x0000_0000),
        b'h' => ("h", 0x00a0_0000),
        b'w' => ("s", 0x0140_0000),
        _ => ("d", 0x01e0_0000),
    };
    let zt = zt
        .strip_prefix('{')
        .and_then(|s| s.strip_suffix('}'))
        .ok_or(AssemblyError::Syntax("Expected one Z register in braces"))?;
    let zt = shaped_as(zt.trim(), 'z', lane)?;
    let pg = pg.to_ascii_lowercase();
    let predicate = if load {
        pg.strip_suffix("/z").ok_or(AssemblyError::Syntax(
            "SVE loads require a zeroing predicate",
        ))?
    } else {
        &pg
    };
    let pg = standalone(predicate, 'p', 7)?;
    let memory = memory
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .ok_or(AssemblyError::Syntax("Expected an SVE memory operand"))?;
    let (base, displacement) = match split_operands(memory)?.as_slice() {
        [base] => (*base, 0),
        [base, displacement, scale]
            if scale
                .to_ascii_lowercase()
                .split_whitespace()
                .eq(["mul", "vl"]) =>
        {
            if !displacement.starts_with('#') {
                return Err(AssemblyError::Syntax(
                    "Expected an SVE immediate displacement",
                ));
            }
            (*base, signed_field(number(displacement, resolve)?, 4)?)
        }
        _ => {
            return Err(AssemblyError::UnsupportedForm(
                "Expected a scalar base and optional MUL VL displacement",
            ));
        }
    };
    let rn = gpr(base)?.address()?.index;
    let opcode = if load { 0xa400_a000 } else { 0xe400_e000 };
    Ok(opcode | kind | (displacement << 16) | (pg << 10) | (rn << 5) | zt)
}
