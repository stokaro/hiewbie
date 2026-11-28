use iced_x86::{
    Decoder, DecoderError, DecoderOptions, Formatter, FormatterOutput, FormatterTextKind,
    Instruction, IntelFormatter, OpKind, Register,
};

use super::{
    BranchTarget, DecodeMode, DecodeStatus, DecodedInstruction, TextBuffer, fallback, instruction,
};
use crate::formats::VirtualAddress;

impl FormatterOutput for TextBuffer {
    fn write(&mut self, text: &str, _kind: FormatterTextKind) {
        self.append(text);
    }
}

pub(super) fn decode(
    bytes: &[u8],
    address: VirtualAddress,
    mode: DecodeMode,
) -> DecodedInstruction {
    // The mode enum constrains the bitness to the decoder's three supported values.
    let mut decoder = Decoder::with_ip(mode.bitness(), bytes, address.get(), DecoderOptions::NONE);
    let decoded = decoder.decode();
    if decoded.is_invalid() || decoded.len() == 0 || decoded.len() > bytes.len() {
        let status = if mode == DecodeMode::X86_64 && recognized_legacy_only(bytes, address) {
            // For an otherwise undefined opcode the backend may report
            // NoMoreBytes at EOF even when the legacy encoding is complete.
            DecodeStatus::InvalidForMode
        } else if decoder.last_error() == DecoderError::NoMoreBytes {
            DecodeStatus::Truncated
        } else {
            DecodeStatus::Unknown
        };
        return fallback(bytes, address, mode, status);
    }
    let direct_branch = near_branch(&decoded).map(|address| BranchTarget { address, mode });
    let mut formatter = IntelFormatter::new();
    formatter.options_mut().set_hex_prefix("0x");
    formatter.options_mut().set_hex_suffix("");
    formatter.options_mut().set_uppercase_hex(false);
    formatter
        .options_mut()
        .set_small_hex_numbers_in_decimal(false);
    let mut text = TextBuffer::new();
    formatter.format(&decoded, &mut text);
    instruction(
        &bytes[..decoded.len()],
        address,
        text.finish(),
        DecodeStatus::Decoded,
        direct_branch,
    )
}

fn recognized_legacy_only(bytes: &[u8], address: VirtualAddress) -> bool {
    // These unprefixed opcodes have no long-mode meaning. Verify a complete
    // legacy instruction before classifying a rejection. Shared opcode space
    // such as BOUND versus EVEX remains Unknown when decoding fails.
    matches!(
        bytes.first(),
        Some(0x27 | 0x2f | 0x37 | 0x3f | 0x60 | 0x61 | 0x9a | 0xce | 0xd4 | 0xd5 | 0xea)
    ) && [32, 16].into_iter().any(|bitness| {
        !Decoder::with_ip(bitness, bytes, address.get(), DecoderOptions::NONE)
            .decode()
            .is_invalid()
    })
}

fn near_branch(instruction: &Instruction) -> Option<VirtualAddress> {
    instruction
        .op_kinds()
        .any(|kind| {
            matches!(
                kind,
                OpKind::NearBranch16 | OpKind::NearBranch32 | OpKind::NearBranch64
            )
        })
        .then(|| VirtualAddress::new(instruction.near_branch_target()))
}

fn has_data_reference(instruction: &Instruction, target: u64) -> bool {
    instruction
        .op_kinds()
        .enumerate()
        .any(|(index, kind)| match kind {
            OpKind::Memory
                if matches!(instruction.segment_prefix(), Register::FS | Register::GS) =>
            {
                false
            }
            OpKind::Memory if instruction.is_ip_rel_memory_operand() => {
                instruction.ip_rel_memory_address() == target
            }
            OpKind::Memory => {
                instruction.memory_base() == Register::None
                    && instruction.memory_index() == Register::None
                    && instruction.memory_displacement64() == target
            }
            OpKind::Immediate16
            | OpKind::Immediate32
            | OpKind::Immediate64
            | OpKind::Immediate32to64 => instruction.immediate(index as u32) == target,
            _ => false,
        })
}

/// Scan instruction starts within `limit`, using any remaining bytes as lookahead.
/// Return the next starting position and the optional match within this buffer.
pub(crate) fn scan_references(
    bytes: &[u8],
    limit: usize,
    address: VirtualAddress,
    mode: DecodeMode,
    target: VirtualAddress,
    mapped: bool,
    bytewise: bool,
) -> (usize, Option<usize>) {
    let mut start = 0;
    while start < limit {
        let Some(ip) = address.get().checked_add(start as u64) else {
            break;
        };
        let mut decoder =
            Decoder::with_ip(mode.bitness(), &bytes[start..], ip, DecoderOptions::NONE);
        let instruction = decoder.decode();
        let valid = !instruction.is_invalid();
        let next = start
            + if valid && !bytewise {
                instruction.len().max(1)
            } else {
                1
            };
        if valid
            && (near_branch(&instruction) == Some(target)
                || (mapped && has_data_reference(&instruction, target.get())))
        {
            return (next, Some(start));
        }
        start = next;
    }
    (start, None)
}
