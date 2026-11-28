use std::fmt::Write;

use yaxpeax_arch::{Decoder, LengthedInstruction, U8Reader};
use yaxpeax_arm::{armv7, armv8::a64};

use super::{
    AddressOperand, BranchTarget, DecodeMode, DecodeStatus, DecodedInstruction, TextBuffer,
    fallback, fallback_with_reason, instruction,
};
use crate::formats::VirtualAddress;

mod a64_extensions;

pub(super) fn decode_arm(
    bytes: &[u8],
    address: VirtualAddress,
    mode: DecodeMode,
) -> DecodedInstruction {
    let mut reader = U8Reader::new(bytes);
    // Permissive decoding constructs an out-of-range paired register for some
    // nonconforming encodings (e.g. d2 ff 20 41). Strict validation must precede
    // that construction. This is a supported backend setting, not panic recovery.
    let decoder = armv7::InstDecoder::default()
        .with_thumb_mode(mode == DecodeMode::Thumb)
        .allow_nonconforming(false);
    let decoded = match decoder.decode(&mut reader) {
        Ok(value) => value,
        Err(error) => {
            let status = if error == armv7::DecodeError::ExhaustedInput {
                DecodeStatus::Truncated
            } else {
                DecodeStatus::Unknown
            };
            return fallback(bytes, address, mode, status);
        }
    };
    let length = decoded.len().to_const() as usize;
    if length == 0 || length > bytes.len() {
        return fallback(bytes, address, mode, DecodeStatus::Unknown);
    }
    let branch = if mode == DecodeMode::Arm {
        arm_branch(&decoded, bytes, address)
    } else {
        thumb_branch(&decoded, bytes, address)
    };
    let mut text = TextBuffer::new();
    let _ = write!(text, "{decoded}");
    let text = pc_relative_text(
        text.finish(),
        branch.map(|target| target.map(|branch| branch.address)),
    );
    instruction(
        &bytes[..length],
        address,
        text,
        DecodeStatus::Decoded,
        branch.flatten(),
    )
}

pub(super) fn decode_aarch64(bytes: &[u8], address: VirtualAddress) -> DecodedInstruction {
    let mode = DecodeMode::Aarch64;
    let Ok(encoding) = bytes.try_into() else {
        return fallback(bytes, address, mode, DecodeStatus::Truncated);
    };
    if let Some(text) = a64_extensions::decode(u32::from_le_bytes(encoding)) {
        return instruction(bytes, address, text, DecodeStatus::Decoded, None);
    }
    let mut reader = U8Reader::new(bytes);
    let decoded = match a64::InstDecoder::default().decode(&mut reader) {
        Ok(value) => value,
        Err(a64::DecodeError::ExhaustedInput) => {
            return fallback(bytes, address, mode, DecodeStatus::Truncated);
        }
        Err(a64::DecodeError::IncompleteDecoder) => {
            return fallback_with_reason(
                bytes,
                address,
                mode,
                DecodeStatus::Unsupported,
                "encoding class unimplemented by AArch64 decoder",
            );
        }
        Err(_) => return fallback(bytes, address, mode, DecodeStatus::Unknown),
    };

    let direct = matches!(
        decoded.opcode,
        a64::Opcode::B
            | a64::Opcode::BL
            | a64::Opcode::Bcc(_)
            | a64::Opcode::BCcc(_)
            | a64::Opcode::CBZ
            | a64::Opcode::CBNZ
            | a64::Opcode::TBZ
            | a64::Opcode::TBNZ
    );
    let literal = matches!(decoded.opcode, a64::Opcode::ADR | a64::Opcode::ADRP);
    let memory = matches!(
        decoded.opcode,
        a64::Opcode::LDR | a64::Opcode::LDRSW | a64::Opcode::PRFM
    );
    let target = if direct || literal || memory {
        decoded.operands.iter().find_map(|operand| {
            let a64::Operand::PCOffset(displacement) = operand else {
                return None;
            };
            let pc = if decoded.opcode == a64::Opcode::ADRP {
                address.get() & !0xfff
            } else {
                address.get()
            };
            Some(
                pc.checked_add_signed(*displacement)
                    .map(VirtualAddress::new),
            )
        })
    } else {
        None
    };
    let mut text = TextBuffer::new();
    let _ = write!(text, "{decoded}");
    let mut row = instruction(
        &bytes[..4],
        address,
        pc_relative_text(text.finish(), target),
        DecodeStatus::Decoded,
        None,
    );
    if let Some(address) = target.flatten() {
        if direct {
            row.direct_branch = Some(BranchTarget { address, mode });
        } else if literal {
            row.address_operand = Some(AddressOperand::Literal(address));
        } else if memory {
            row.address_operand = Some(AddressOperand::Memory(address));
        }
    }
    row
}

/// The backend's ARM/Thumb operand offsets include inconsistent PC biases.
/// Extract direct-branch displacement from the original encoding instead;
/// register-indirect control flow is deliberately left unresolved.
fn arm_branch(
    decoded: &armv7::Instruction,
    bytes: &[u8],
    address: VirtualAddress,
) -> Option<Option<BranchTarget>> {
    if !matches!(
        decoded.opcode,
        armv7::Opcode::B | armv7::Opcode::BL | armv7::Opcode::BLX
    ) {
        return None;
    }
    let word = u32::from_le_bytes(bytes[..4].try_into().ok()?);
    if word & 0x0e00_0000 != 0x0a00_0000 {
        return None;
    }
    let displacement = sign_extend(word & 0x00ff_ffff, 24) * 4 + 8;
    if word >> 28 == 15 {
        Some(relative_arm32_target(
            address,
            displacement + i64::from((word >> 24) & 1) * 2,
            DecodeMode::Thumb,
        ))
    } else {
        Some(relative_arm32_target(
            address,
            displacement,
            DecodeMode::Arm,
        ))
    }
}

fn thumb_branch(
    decoded: &armv7::Instruction,
    bytes: &[u8],
    address: VirtualAddress,
) -> Option<Option<BranchTarget>> {
    if !matches!(
        decoded.opcode,
        armv7::Opcode::B
            | armv7::Opcode::BL
            | armv7::Opcode::BLX
            | armv7::Opcode::CBZ
            | armv7::Opcode::CBNZ
    ) {
        return None;
    }
    let first = u16::from_le_bytes(bytes[..2].try_into().ok()?) as u32;
    if !decoded.wide() {
        let displacement = if first & 0xf800 == 0xe000 {
            sign_extend(first & 0x7ff, 11) * 2
        } else if first & 0xf000 == 0xd000 && (first >> 8) & 15 < 14 {
            sign_extend(first & 0xff, 8) * 2
        } else if first & 0xf500 == 0xb100 {
            i64::from(((first >> 3) & 31) | ((first >> 4) & 32)) * 2
        } else {
            return None;
        };
        return Some(relative_arm32_target(
            address,
            displacement + 4,
            DecodeMode::Thumb,
        ));
    }
    let second = u16::from_le_bytes(bytes[2..4].try_into().ok()?) as u32;
    if first & 0xf800 != 0xf000 {
        return None;
    }
    let s = (first >> 10) & 1;
    let j1 = (second >> 13) & 1;
    let j2 = (second >> 11) & 1;
    if second & 0xd000 == 0x8000 && (first >> 6) & 15 < 14 {
        let encoded =
            (s << 20) | (j2 << 19) | (j1 << 18) | ((first & 63) << 12) | ((second & 0x7ff) << 1);
        return Some(relative_arm32_target(
            address,
            sign_extend(encoded, 21) + 4,
            DecodeMode::Thumb,
        ));
    }
    let kind = second & 0xd000;
    if kind != 0x9000 && kind != 0xd000 && !(kind == 0xc000 && second & 1 == 0) {
        return None;
    }
    let i1 = 1 ^ (j1 ^ s);
    let i2 = 1 ^ (j2 ^ s);
    let encoded =
        (s << 24) | (i1 << 23) | (i2 << 22) | ((first & 0x3ff) << 12) | ((second & 0x7ff) << 1);
    let displacement = sign_extend(encoded, 25);
    if kind == 0xc000 {
        // Thumb BLX aligns the architectural PC before switching to ARM state.
        let pc = u32::try_from(address.get())
            .ok()
            .map(|value| VirtualAddress::new(u64::from(value.wrapping_add(4) & !3)));
        Some(pc.and_then(|pc| relative_arm32_target(pc, displacement, DecodeMode::Arm)))
    } else {
        Some(relative_arm32_target(
            address,
            displacement + 4,
            DecodeMode::Thumb,
        ))
    }
}

/// A32/T32 PC arithmetic is modulo 2^32, independent of the 64-bit file offset.
/// Reject an interpretation whose starting VA does not fit that address space.
fn relative_arm32_target(
    address: VirtualAddress,
    displacement: i64,
    mode: DecodeMode,
) -> Option<BranchTarget> {
    let address = u32::try_from(address.get()).ok()?;
    let displacement = i32::try_from(displacement).ok()?;
    Some(BranchTarget {
        address: VirtualAddress::new(u64::from(address.wrapping_add_signed(displacement))),
        mode,
    })
}

fn sign_extend(value: u32, bits: u32) -> i64 {
    i64::from((value << (32 - bits)) as i32 >> (32 - bits))
}

fn pc_relative_text(mut text: String, target: Option<Option<VirtualAddress>>) -> String {
    if let Some(target) = target
        && let Some(start) = text.find('$')
    {
        text.truncate(start);
        if let Some(target) = target {
            let _ = write!(text, "0x{:x}", target.get());
        } else {
            text.push_str("<address out of range>");
        }
        // Branch replacements are small, but preserve the formatter budget.
        let mut bounded = TextBuffer::new();
        bounded.append(&text);
        return bounded.finish();
    }
    text
}
