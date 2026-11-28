//! Instruction boundaries and spellings observed in HIEW 8.63.

use iced_x86::Code;

use super::{DecodeMode, InstructionProfile, MAX_INSTRUCTION_BYTES};

pub(super) fn rejected_row(
    bytes: &[u8],
    profile: InstructionProfile,
) -> Option<(usize, &'static str)> {
    if !profile.hiew_x86 {
        return None;
    }
    let bytes = &bytes[..bytes.len().min(MAX_INSTRUCTION_BYTES)];
    let long = profile.mode == DecodeMode::X86_64;
    let start = bytes.iter().position(|&byte| {
        let legacy_prefix = matches!(
            byte,
            0x26 | 0x2e | 0x36 | 0x3e | 0x64 | 0x65 | 0x66 | 0x67 | 0xf0 | 0xf2 | 0xf3
        );
        let rex_prefix = long && (0x40..=0x4f).contains(&byte);
        !(legacy_prefix || rex_prefix)
    })?;
    let (prefixes, opcode) = bytes.split_at(start);
    let legacy_mod_error = matches!(opcode, [0x62, 0xc0..=0xff, ..])
        || (profile.mode == DecodeMode::X86_16 && matches!(opcode, [0xc4 | 0xc5, 0xc0..=0xff, ..]));

    let (length, reason) = if long && opcode[0] == 0x62 {
        (1, "#UD(64)")
    } else if legacy_mod_error {
        // BOUND, LES, and LDS require memory. The original leaves the
        // register-only ModR/M byte at the start of the next row.
        (
            1,
            if prefixes.contains(&0xf0) {
                "#UD(lock)"
            } else {
                "#UD(mod)"
            },
        )
    } else if profile.mode != DecodeMode::X86_16 {
        wide_rejection(opcode, prefixes, long)?
    } else {
        return None;
    };
    Some((start + length, reason))
}

fn wide_rejection(opcode: &[u8], prefixes: &[u8], long: bool) -> Option<(usize, &'static str)> {
    match opcode {
        // HIEW consumes the invalid POP group selector, including XOP maps.
        [0x8f, _, ..] if prefixes.contains(&0xf0) => Some((1, "#UD(lock)")),
        [0x8f, modrm, ..] if modrm & 0x38 != 0 => Some((2, "#UD")),
        [0xc5, flags, ..] if long || flags & 0xc0 == 0xc0 => {
            vex_rejection(opcode, prefixes, 1, *flags, 2)
        }
        [0xc4, map, flags, ..] if long || map & 0xc0 == 0xc0 => {
            vex_rejection(opcode, prefixes, map & 0x1f, *flags, 3)
        }
        [0x0f, 0x01, 0xd5 | 0xd6 | 0xee | 0xef, ..] => Some((3, "#UD")),
        [0x0f, 0x38, 0xc8 | 0xcb | 0xcd, ..] => Some((3, "#UD")),
        [0x0f, 0x38, 0xf6, ..] if prefixes.contains(&0x66) || has_rep(prefixes) => Some((3, "#UD")),
        [0x0f, 0xae, 0xc0..=0xdf, ..] if has_rep(prefixes) => Some((3, "#UD")),
        _ => None,
    }
}

fn has_rep(prefixes: &[u8]) -> bool {
    prefixes
        .iter()
        .rev()
        .find(|&&byte| matches!(byte, 0xf2 | 0xf3))
        == Some(&0xf3)
}

fn vex_rejection(
    opcode: &[u8],
    prefixes: &[u8],
    map: u8,
    flags: u8,
    header: usize,
) -> Option<(usize, &'static str)> {
    if prefixes
        .iter()
        .any(|&byte| matches!(byte, 0x40..=0x4f | 0x66 | 0xf0 | 0xf2 | 0xf3))
    {
        return Some((header, "#UD()"));
    }
    let operation = *opcode.get(header)?;
    let mandatory = flags & 3;
    let wide_vector = flags & 4 != 0;
    // Preserve the validated 128-bit forms and opcode neighbors. The
    // unavailable forms below stop at the opcode, before the ModR/M byte.
    let reason = match (map, operation, mandatory) {
        (1, 0xef | 0xfe, 1) | (2, 0x00, 1) if wide_vector => Some("#UD(vex.L)"),
        (2, 0x58, 1) | (3, 0x46, 1) | (2, 0xf2, 0) | (2, 0xf5, 0 | 2 | 3) => Some("#UD"),
        _ => None,
    }?;
    Some((header + 1, reason))
}

pub(super) fn display_code(code: Code, profile: InstructionProfile) -> Code {
    if !profile.hiew_x86 || profile.mode == DecodeMode::X86_16 {
        return code;
    }
    match code {
        Code::Tzcnt_r16_rm16 => Code::Bsf_r16_rm16,
        Code::Tzcnt_r32_rm32 => Code::Bsf_r32_rm32,
        Code::Tzcnt_r64_rm64 => Code::Bsf_r64_rm64,
        Code::Clflushopt_m8 => Code::Clflush_m8,
        _ => code,
    }
}

#[derive(Default)]
pub(super) struct PrefixOverride {
    pub ignored: usize,
    pub rejection: Option<&'static str>,
}

pub(super) fn memory_prefix(bytes: &[u8], profile: InstructionProfile) -> PrefixOverride {
    let [0xf3, 0x0f, 0xae, modrm, ..] = bytes else {
        return PrefixOverride::default();
    };
    if !profile.hiew_x86 || profile.mode == DecodeMode::X86_16 || *modrm >= 0xc0 {
        return PrefixOverride::default();
    }
    let rejection = match modrm & 0x38 {
        // FXSAVE, FXRSTOR, LDMXCSR, and STMXCSR retain their legacy meanings.
        0x00..=0x18 => None,
        // For XSAVE and XRSTOR, HIEW consumes the complete memory operand
        // before reporting the prefix error.
        0x20 | 0x28 => Some("#UD()"),
        _ => return PrefixOverride::default(),
    };
    PrefixOverride {
        ignored: 1,
        rejection,
    }
}
