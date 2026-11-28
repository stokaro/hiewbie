use iced_x86::{
    Code, CpuidFeature, Decoder, DecoderOptions, Encoder, EncodingKind, Instruction, Mnemonic,
    OpCodeInfo, OpCodeOperandKind as Kind, OpKind, Register,
};

use super::AssemblyError;
use super::syntax::{BranchForm, Memory, Operand, Prefix, Source};
use crate::formats::VirtualAddress;
use crate::instructions::DecodeMode;

enum BindError {
    Mismatch,
    Range(&'static str),
}

pub(super) fn encode(
    source: &Source,
    mode: DecodeMode,
    address: VirtualAddress,
) -> Result<Vec<Vec<u8>>, AssemblyError> {
    let bitness = mode.bitness();
    let mut encodings = Vec::new();
    let mut range_error = None;
    let ip = address.get();
    if source.mnemonic == Mnemonic::Nop
        && source.operands.is_empty()
        && matches!(source.prefix, Prefix::None)
    {
        return nop_encodings(bitness, ip);
    }
    let mut supported_opcode = false;
    let mut memory_width = None;
    for code in Code::values().filter(|code| code.mnemonic() == source.mnemonic) {
        let info = code.op_code();
        if !info.is_instruction()
            || !info.is_available_in_mode(bitness)
            || !supported_family(code, info, bitness)
        {
            continue;
        }
        supported_opcode = true;
        let mut instruction = Instruction::default();
        instruction.set_code(code);
        let width = info.memory_size().size();
        let branch = match bind(&mut instruction, source, info, bitness) {
            Ok(branch) => branch,
            Err(BindError::Mismatch) => continue,
            Err(BindError::Range(reason)) => {
                range_error = Some(reason);
                continue;
            }
        };
        let rip_displacement = instruction
            .is_ip_rel_memory_operand()
            .then(|| instruction.memory_displacement64() as i64);
        if rip_displacement.is_some() {
            instruction.set_memory_displacement64(ip);
        }
        let mut encoder = Encoder::new(bitness);
        let Ok(length) = encoder.encode(&instruction, ip) else {
            if branch.is_some() {
                range_error = Some("Branch target is outside the selected displacement width");
            }
            continue;
        };
        let Some(end) = ip.checked_add(length as u64) else {
            range_error = Some("Instruction end exceeds the virtual address space");
            continue;
        };
        let mut bytes = encoder.take_buffer();
        if let Some(displacement) = rip_displacement {
            let Ok(target) = u64::try_from(i128::from(end) + i128::from(displacement)) else {
                range_error = Some("RIP-relative address exceeds the virtual address space");
                continue;
            };
            instruction.set_memory_displacement64(target);
            if !matches!(encoder.encode(&instruction, ip), Ok(size) if size == length) {
                range_error = Some("RIP-relative displacement could not be encoded");
                continue;
            }
            bytes = encoder.take_buffer();
        }
        if let Some((target, bits)) = branch {
            let displacement = i128::from(target) - i128::from(end);
            if !fits_signed(displacement, bits) {
                range_error = Some("Branch target is outside the selected displacement width");
                continue;
            }
        }
        // Reject reserved encodings and aliases that decode as another mnemonic,
        // such as the accumulator XCHG opcode that is treated as NOP.
        let decoded = Decoder::with_ip(bitness, &bytes, ip, DecoderOptions::NONE).decode();
        if decoded.len() != bytes.len() || decoded.mnemonic() != source.mnemonic {
            continue;
        }
        if width != 0
            && source
                .operands
                .iter()
                .any(|op| matches!(op, Operand::Memory(_)))
            && memory_width
                .replace(width)
                .is_some_and(|previous| previous != width)
        {
            return Err(AssemblyError::AmbiguousMemorySize);
        }
        encodings.push(bytes);
    }
    if encodings.is_empty() {
        return Err(if let Some(reason) = range_error {
            AssemblyError::OutOfRange(reason)
        } else if supported_opcode {
            AssemblyError::UnsupportedForm("Unsupported operand form or instruction prefix")
        } else {
            AssemblyError::UnsupportedForm("Instruction family is unavailable in this mode")
        });
    }
    encodings.sort_by(|a, b| (a.len(), a).cmp(&(b.len(), b)));
    encodings.dedup();
    Ok(encodings)
}

fn nop_encodings(bitness: u32, ip: u64) -> Result<Vec<Vec<u8>>, AssemblyError> {
    // CommandSelect in HIEW offers these nine sequences at 32/64 bits.
    // 16-bit address decoding changes the multi-byte ModR/M lengths.
    const SEQUENCES: &[&[u8]] = &[
        &[0x90],
        &[0x66, 0x90],
        &[0x0F, 0x1F, 0x00],
        &[0x0F, 0x1F, 0x40, 0],
        &[0x0F, 0x1F, 0x44, 0, 0],
        &[0x66, 0x0F, 0x1F, 0x44, 0, 0],
        &[0x0F, 0x1F, 0x80, 0, 0, 0, 0],
        &[0x0F, 0x1F, 0x84, 0, 0, 0, 0, 0],
        &[0x66, 0x0F, 0x1F, 0x84, 0, 0, 0, 0, 0],
    ];
    let count = if bitness == 16 { 1 } else { SEQUENCES.len() };
    let choices: Vec<_> = SEQUENCES[..count]
        .iter()
        .filter(|bytes| ip.checked_add(bytes.len() as u64).is_some())
        .map(|bytes| bytes.to_vec())
        .collect();
    if choices.is_empty() {
        return Err(AssemblyError::OutOfRange(
            "Instruction end exceeds the virtual address space",
        ));
    }
    Ok(choices)
}

fn supported_family(code: Code, info: &OpCodeInfo, bitness: u32) -> bool {
    use CpuidFeature::*;
    let features = code.cpuid_features();
    match info.encoding() {
        EncodingKind::Legacy => features.iter().all(|feature| {
            matches!(
                feature,
                INTEL8086
                    | INTEL186
                    | INTEL286
                    | INTEL386
                    | INTEL486
                    | X64
                    | CMOV
                    | CPUID
                    | CX8
                    | CMPXCHG16B
                    | FPU
                    | FPU287
                    | FPU387
                    | FXSR
                    | MMX
                    | SSE
                    | SSE2
                    | SSE3
                    | SSSE3
                    | SSE4_1
                    | SSE4_2
                    | AES
                    | PCLMULQDQ
                    | POPCNT
                    | LZCNT
                    | CLFSH
                    | SEP
                    | SYSCALL
                    | MULTIBYTENOP
                    | PAUSE
                    | TSC
                    | RDTSCP
                    | XSAVE
                    | XSAVEOPT
            )
        }),
        EncodingKind::VEX => bitness != 16 && features == [AVX],
        _ => false,
    }
}

fn bind(
    instruction: &mut Instruction,
    source: &Source,
    info: &OpCodeInfo,
    bitness: u32,
) -> Result<Option<(u64, u32)>, BindError> {
    use Kind::*;
    let kinds = info.op_kinds();
    let width_selected = source.operands.iter().any(|operand| match operand {
        Operand::Register(register) => register.is_gpr(),
        Operand::Memory(memory) => memory.size.is_some(),
        _ => false,
    });
    let operand_size = info.operand_size();
    if !width_selected
        && operand_size != 0
        && operand_size != bitness
        && matches!(
            source.mnemonic,
            Mnemonic::Push | Mnemonic::Pop | Mnemonic::Ret | Mnemonic::Retf
        )
    {
        return Err(BindError::Mismatch);
    }
    match source.prefix {
        Prefix::None => {}
        Prefix::Lock
            if info.can_use_lock_prefix()
                && matches!(source.operands.first(), Some(Operand::Memory(_))) =>
        {
            instruction.set_has_lock_prefix(true);
        }
        Prefix::Rep if info.can_use_rep_prefix() => instruction.set_has_rep_prefix(true),
        Prefix::Repne if info.can_use_repne_prefix() => instruction.set_has_repne_prefix(true),
        _ => return Err(BindError::Mismatch),
    }
    if source.operands.is_empty() && !kinds.is_empty() {
        let address_size = info.address_size();
        if address_size != 0 && address_size != bitness {
            return Err(BindError::Mismatch);
        }
        for (index, &kind) in kinds.iter().enumerate() {
            let op = match (kind, bitness) {
                (seg_rSI, 16) => OpKind::MemorySegSI,
                (seg_rSI, 32) => OpKind::MemorySegESI,
                (seg_rSI, _) => OpKind::MemorySegRSI,
                (es_rDI, 16) => OpKind::MemoryESDI,
                (es_rDI, 32) => OpKind::MemoryESEDI,
                (es_rDI, _) => OpKind::MemoryESRDI,
                _ => return Err(BindError::Mismatch),
            };
            instruction.set_op_kind(index as u32, op);
        }
        return Ok(Option::None);
    }
    if source.operands.len() != kinds.len() {
        return Err(BindError::Mismatch);
    }
    if source.branch_form == BranchForm::FarMemory
        && (!instruction.is_jmp_far_indirect() && !instruction.is_call_far_indirect())
    {
        return Err(BindError::Mismatch);
    }
    if source.branch_form != BranchForm::FarMemory
        && (instruction.is_jmp_far_indirect() || instruction.is_call_far_indirect())
    {
        return Err(BindError::Mismatch);
    }
    let mut branch = Option::None;
    for (index, (&kind, &operand)) in kinds.iter().zip(&source.operands).enumerate() {
        let index = index as u32;
        let relative = match kind {
            br16_1 => Some((16, 8)),
            br32_1 => Some((32, 8)),
            br64_1 => Some((64, 8)),
            br16_2 => Some((16, 16)),
            br32_4 => Some((32, 32)),
            br64_4 => Some((64, 32)),
            _ => std::option::Option::None,
        };
        if let Some((ip_bits, displacement_bits)) = relative {
            let short = displacement_bits == 8;
            if ip_bits != bitness
                || (source.mnemonic == Mnemonic::Jmp
                    && short != (source.branch_form == BranchForm::Short))
            {
                return Err(BindError::Mismatch);
            }
            let Operand::Number(value) = operand else {
                return Err(BindError::Mismatch);
            };
            let target = u64::try_from(value).map_err(|_| {
                BindError::Range("Branch target is outside the virtual address space")
            })?;
            // Short and near 16/32-bit encodings use the low architectural IP.
            // The caller also checks the displacement in the full linear address space.
            match ip_bits {
                16 => {
                    instruction.set_op_kind(index, OpKind::NearBranch16);
                    instruction.set_near_branch16(target as u16);
                }
                32 => {
                    instruction.set_op_kind(index, OpKind::NearBranch32);
                    instruction.set_near_branch32(target as u32);
                }
                _ => {
                    instruction.set_op_kind(index, OpKind::NearBranch64);
                    instruction.set_near_branch64(target);
                }
            }
            branch = Some((target, displacement_bits));
            continue;
        }
        if source.branch_form == BranchForm::Short {
            return Err(BindError::Mismatch);
        }
        match operand {
            Operand::Register(register) => {
                if !register_matches(kind, register) {
                    return Err(BindError::Mismatch);
                }
                instruction.set_op_register(index, register);
            }
            Operand::Number(value) => immediate(instruction, index, kind, value)?,
            Operand::Memory(memory) => set_memory(instruction, index, kind, info, memory, bitness)?,
            Operand::Far { selector, offset } => {
                if (kind == farbr2_2 && bitness != 16) || (kind == farbr4_2 && bitness != 32) {
                    return Err(BindError::Mismatch);
                }
                set_far(instruction, index, kind, selector, offset)?;
            }
        }
    }
    Ok(branch)
}

fn register_matches(kind: Kind, register: Register) -> bool {
    use Kind::*;
    match kind {
        r8_or_mem | r8_reg | r8_opcode => register.is_gpr8(),
        r16_or_mem | r16_reg | r16_rm | r16_opcode => register.is_gpr16(),
        r32_or_mem | r32_reg | r32_rm | r32_opcode | r32_vvvv => register.is_gpr32(),
        r64_or_mem | r64_reg | r64_rm | r64_opcode | r64_vvvv => register.is_gpr64(),
        seg_reg => register.is_segment_register(),
        mm_or_mem | mm_reg | mm_rm => register.is_mm(),
        xmm_or_mem | xmm_reg | xmm_rm | xmm_vvvv | xmm_is4 => register.is_xmm(),
        ymm_or_mem | ymm_reg | ymm_rm | ymm_vvvv | ymm_is4 => register.is_ymm(),
        cr_reg => register.is_cr(),
        dr_reg => register.is_dr(),
        tr_reg => register.is_tr(),
        sti_opcode => register.is_st(),
        es => register == Register::ES,
        cs => register == Register::CS,
        ss => register == Register::SS,
        ds => register == Register::DS,
        fs => register == Register::FS,
        gs => register == Register::GS,
        al => register == Register::AL,
        cl => register == Register::CL,
        ax => register == Register::AX,
        dx => register == Register::DX,
        eax => register == Register::EAX,
        rax => register == Register::RAX,
        st0 => register == Register::ST0,
        _ => false,
    }
}

fn set_memory(
    instruction: &mut Instruction,
    index: u32,
    kind: Kind,
    info: &OpCodeInfo,
    memory: Memory,
    bitness: u32,
) -> Result<(), BindError> {
    use Kind::*;
    if !matches!(
        kind,
        mem_offs
            | mem
            | r8_or_mem
            | r16_or_mem
            | r32_or_mem
            | r64_or_mem
            | mm_or_mem
            | xmm_or_mem
            | ymm_or_mem
    ) || memory
        .size
        .is_some_and(|size| size != info.memory_size().size())
    {
        return Err(BindError::Mismatch);
    }
    if memory.base == Register::RIP {
        if bitness != 64 || kind == mem_offs || memory.index != Register::None {
            return Err(BindError::Mismatch);
        }
        if !fits_signed(memory.displacement, 32) {
            return Err(BindError::Range(
                "RIP-relative displacement does not fit 32 bits",
            ));
        }
        instruction.set_op_kind(index, OpKind::Memory);
        instruction.set_memory_base(Register::RIP);
        instruction.set_segment_prefix(memory.segment);
        instruction.set_memory_displ_size(8);
        instruction.set_memory_displacement64(memory.displacement as u64);
        return Ok(());
    }
    let mut address_bits = bitness;
    for reg in [memory.base, memory.index] {
        if reg != Register::None {
            address_bits = (reg.size() * 8) as u32;
            break;
        }
    }
    if address_bits < 16
        || (bitness == 64 && address_bits == 16)
        || (bitness != 64 && address_bits == 64)
        || (memory.index != Register::None && memory.index.size() * 8 != address_bits as usize)
        || (address_bits == 16 && memory.scale != 1)
        || (kind == mem_offs && (memory.base != Register::None || memory.index != Register::None))
    {
        return Err(BindError::Mismatch);
    }
    let displacement = memory.displacement;
    if address_bits == 64 && kind != mem_offs {
        if !fits_bits(displacement, 64) || !same_sign_extended_bits(displacement, 32, 64) {
            return Err(BindError::Range(
                "Memory displacement does not fit a signed 32-bit field",
            ));
        }
    } else if !fits_bits(displacement, address_bits) {
        return Err(BindError::Range(
            "Memory address does not fit the selected address size",
        ));
    }
    instruction.set_op_kind(index, OpKind::Memory);
    instruction.set_memory_base(memory.base);
    instruction.set_memory_index(memory.index);
    instruction.set_memory_index_scale(memory.scale);
    instruction.set_segment_prefix(memory.segment);
    let displacement_mask = ((1_i128 << address_bits) - 1) as u64;
    instruction.set_memory_displacement64(displacement as u64 & displacement_mask);
    instruction.set_memory_displ_size(
        if memory.base == Register::None && memory.index == Register::None {
            address_bits / 8
        } else if displacement == 0 {
            0
        } else if same_sign_extended_bits(displacement, 8, address_bits) {
            1
        } else {
            address_bits / 8
        },
    );
    Ok(())
}

fn set_far(
    instruction: &mut Instruction,
    index: u32,
    kind: Kind,
    selector: i128,
    offset: i128,
) -> Result<(), BindError> {
    match kind {
        Kind::farbr2_2 => {
            let offset = u16::try_from(offset)
                .map_err(|_| BindError::Range("Far offset does not fit 16 bits"))?;
            instruction.set_op_kind(index, OpKind::FarBranch16);
            instruction.set_far_branch16(offset);
        }
        Kind::farbr4_2 => {
            let offset = u32::try_from(offset)
                .map_err(|_| BindError::Range("Far offset does not fit 32 bits"))?;
            instruction.set_op_kind(index, OpKind::FarBranch32);
            instruction.set_far_branch32(offset);
        }
        _ => return Err(BindError::Mismatch),
    }
    let selector = u16::try_from(selector)
        .map_err(|_| BindError::Range("Far selector does not fit 16 bits"))?;
    instruction.set_far_branch_selector(selector);
    Ok(())
}

fn immediate(
    instruction: &mut Instruction,
    index: u32,
    kind: Kind,
    value: i128,
) -> Result<(), BindError> {
    use Kind::*;
    let (op, bits, width) = match kind {
        imm8 => {
            let op = if instruction
                .op_kinds()
                .take(index as usize)
                .any(|kind| kind == OpKind::Immediate16)
            {
                OpKind::Immediate8_2nd
            } else {
                OpKind::Immediate8
            };
            (op, 8, 8)
        }
        imm8_const_1 if value == 1 => (OpKind::Immediate8, 8, 8),
        imm16 => (OpKind::Immediate16, 16, 16),
        imm32 => (OpKind::Immediate32, 32, 32),
        imm64 => (OpKind::Immediate64, 64, 64),
        imm8sex16 => (OpKind::Immediate8to16, 8, 16),
        imm8sex32 => (OpKind::Immediate8to32, 8, 32),
        imm8sex64 => (OpKind::Immediate8to64, 8, 64),
        imm32sex64 => (OpKind::Immediate32to64, 32, 64),
        _ => return Err(BindError::Mismatch),
    };
    if !fits_bits(value, width) || (bits < width && !same_sign_extended_bits(value, bits, width)) {
        return Err(BindError::Range(
            "Immediate is outside the selected operand width",
        ));
    }
    instruction.set_op_kind(index, op);
    instruction
        .try_set_immediate_u64(index, value as u64)
        .map_err(|_| BindError::Mismatch)
}

fn fits_signed(value: i128, bits: u32) -> bool {
    let sign = 1_i128 << (bits - 1);
    (-sign..sign).contains(&value)
}

fn fits_bits(value: i128, bits: u32) -> bool {
    (-(1_i128 << (bits - 1))..(1_i128 << bits)).contains(&value)
}

fn same_sign_extended_bits(value: i128, bits: u32, width: u32) -> bool {
    let sign = 1_i128 << (bits - 1);
    let narrow = (value & (sign * 2 - 1)) ^ sign;
    let mask = (1_i128 << width) - 1;
    (narrow - sign) & mask == value & mask
}
