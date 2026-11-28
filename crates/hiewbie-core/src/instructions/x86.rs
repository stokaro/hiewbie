use iced_x86::{
    Decoder, DecoderError, DecoderOptions, Formatter, FormatterOutput, FormatterTextKind,
    Instruction, IntelFormatter, Mnemonic, OpKind, Register,
};

use super::{
    AddressOperand, BranchTarget, DecodeMode, DecodeStatus, DecodedInstruction, InstructionProfile,
    MAX_INSTRUCTION_BYTES, TextBuffer, fallback, instruction,
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
    profile: InstructionProfile,
) -> DecodedInstruction {
    if let Some((length, message)) = hiew_legacy_rejection(bytes, profile) {
        return instruction(
            &bytes[..length],
            address,
            message.to_owned(),
            DecodeStatus::InvalidEncoding,
            None,
        );
    }
    let mode = profile.mode;
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
    let mut result = instruction(
        &bytes[..decoded.len()],
        address,
        text.finish(),
        DecodeStatus::Decoded,
        direct_branch,
    );
    result.address_operand = address_operand(&decoded);
    result.x86_displacement_bytes = decoder.get_constant_offsets(&decoded).displacement_size();
    result
}

fn hiew_legacy_rejection(
    bytes: &[u8],
    profile: InstructionProfile,
) -> Option<(usize, &'static str)> {
    if !profile.hiew_x86 || profile.mode != DecodeMode::X86_16 {
        return None;
    }
    let bytes = &bytes[..bytes.len().min(MAX_INSTRUCTION_BYTES)];
    let opcode = bytes.iter().position(|&byte| {
        !matches!(
            byte,
            0x26 | 0x2e | 0x36 | 0x3e | 0x64 | 0x65 | 0x66 | 0x67 | 0xf0 | 0xf2 | 0xf3
        )
    })?;
    // These leads select LES, LDS, or BOUND in HIEW 8.63's 16-bit view.
    // Keep prefixes and the opcode in the rejected row. The next row
    // starts at the register-only ModR/M byte.
    if !matches!(&bytes[opcode..], [0xc4 | 0xc5 | 0x62, 0xc0..=0xff, ..]) {
        return None;
    }
    let message = if bytes[..opcode].contains(&0xf0) {
        "#UD(lock)"
    } else {
        "#UD(mod)"
    };
    Some((opcode + 1, message))
}

fn static_memory_address(instruction: &Instruction) -> Option<VirtualAddress> {
    if matches!(instruction.segment_prefix(), Register::FS | Register::GS) {
        return None;
    }
    if instruction.is_ip_rel_memory_operand() {
        return Some(VirtualAddress::new(instruction.ip_rel_memory_address()));
    }
    (instruction.memory_base() == Register::None && instruction.memory_index() == Register::None)
        .then(|| VirtualAddress::new(instruction.memory_displacement64()))
}

fn address_operand(instruction: &Instruction) -> Option<AddressOperand> {
    if instruction.op_kinds().any(|kind| kind == OpKind::Memory) {
        let address = static_memory_address(instruction)?;
        if instruction.is_jmp_near_indirect() || instruction.is_call_near_indirect() {
            return Some(AddressOperand::Indirect {
                slot: address,
                width: instruction.memory_size().size(),
            });
        }
        return Some(AddressOperand::Memory(address));
    }
    if instruction.mnemonic() == Mnemonic::Mov
        && instruction.op_count() == 2
        && matches!(
            instruction.op1_kind(),
            OpKind::Immediate16
                | OpKind::Immediate32
                | OpKind::Immediate64
                | OpKind::Immediate32to64
        )
    {
        return Some(AddressOperand::Literal(VirtualAddress::new(
            instruction.immediate(1),
        )));
    }
    None
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
            OpKind::Memory => {
                static_memory_address(instruction) == Some(VirtualAddress::new(target))
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
    profile: InstructionProfile,
    target: VirtualAddress,
    mapped: bool,
    bytewise: bool,
) -> (usize, Option<usize>) {
    let mut start = 0;
    while start < limit {
        let Some(ip) = address.get().checked_add(start as u64) else {
            break;
        };
        if let Some((length, _)) = hiew_legacy_rejection(&bytes[start..], profile) {
            start += if bytewise { 1 } else { length };
            continue;
        }
        let mut decoder = Decoder::with_ip(
            profile.mode.bitness(),
            &bytes[start..],
            ip,
            DecoderOptions::NONE,
        );
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
