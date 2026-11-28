//! Reproducible text-to-bytes experiment. This crate is outside the product workspace.

use iced_x86::{Code, Encoder, Instruction, Register};

#[derive(Clone, Copy, Debug)]
pub enum Profile {
    X86_16,
    X86_32,
    X86_64,
    Aarch64,
}

pub fn assemble(source: &str, profile: Profile, pc: u64) -> Result<Vec<Vec<u8>>, &'static str> {
    if source.len() > 128 || !source.is_ascii() || source.contains(['\r', '\n']) {
        return Err("expected one bounded ASCII instruction");
    }
    let source = source.trim().to_ascii_lowercase();
    let (mnemonic, operands) = source.split_once(' ').unwrap_or((&source, ""));
    let operands = operands.trim();
    if matches!(profile, Profile::Aarch64) {
        return assemble_aarch64(mnemonic, operands, pc)
            .map(|word| vec![word.to_le_bytes().to_vec()]);
    }
    assemble_x86(mnemonic, operands, profile, pc)
}

fn number(token: &str) -> Result<u64, &'static str> {
    let token = token.trim();
    let (digits, radix) = if let Some(hex) = token.strip_prefix("0x") {
        (hex, 16)
    } else if let Some(hex) = token.strip_suffix('h') {
        (hex, 16)
    } else if let Some(decimal) = token.strip_suffix('t') {
        (decimal, 10)
    } else {
        (token, 16)
    };
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("invalid number or unresolved symbol");
    }
    u64::from_str_radix(digits, radix).map_err(|_| "invalid or overflowing number")
}

fn assemble_x86(
    mnemonic: &str,
    operands: &str,
    profile: Profile,
    pc: u64,
) -> Result<Vec<Vec<u8>>, &'static str> {
    let (bitness, short, near) = match profile {
        Profile::X86_16 => (16, Code::Jmp_rel8_16, Code::Jmp_rel16),
        Profile::X86_32 => (32, Code::Jmp_rel8_32, Code::Jmp_rel32_32),
        Profile::X86_64 => (64, Code::Jmp_rel8_64, Code::Jmp_rel32_64),
        Profile::Aarch64 => return Err("expected an x86 profile"),
    };
    let fits_address = |value: u64| bitness == 64 || value < (1_u64 << bitness);
    if !fits_address(pc) {
        return Err("address outside the selected profile");
    }
    let instructions = match mnemonic {
        "nop" if operands.is_empty() => vec![Instruction::with(Code::Nopd)],
        "mov" => {
            let (register, immediate) = operands
                .split_once(',')
                .ok_or("expected register, immediate")?;
            let immediate = number(immediate)?;
            let instruction = match register.trim() {
                "ax" if immediate <= u64::from(u16::MAX) => {
                    Instruction::with2(Code::Mov_r16_imm16, Register::AX, immediate as u32)
                }
                "eax" if immediate <= u64::from(u32::MAX) => {
                    Instruction::with2(Code::Mov_r32_imm32, Register::EAX, immediate as u32)
                }
                "rax" if bitness == 64 => {
                    Instruction::with2(Code::Mov_r64_imm64, Register::RAX, immediate)
                }
                _ => return Err("register or immediate outside the supported form"),
            };
            vec![instruction.map_err(|_| "invalid MOV operands")?]
        }
        "jmp" | "jmps" => {
            let (qualifier, target) = match operands.split_once(' ') {
                Some((width @ ("short" | "near"), target)) => (width, number(target)?),
                _ if mnemonic == "jmps" => ("short", number(operands)?),
                _ => ("", number(operands)?),
            };
            if !fits_address(target) {
                return Err("branch target outside the selected profile");
            }
            let forms = match qualifier {
                "short" => &[short][..],
                "near" => &[near][..],
                _ => &[short, near][..],
            };
            forms
                .iter()
                .filter_map(|&code| Instruction::with_branch(code, target).ok())
                .collect()
        }
        "vaddps"
            if bitness == 64
                && operands
                    .split(',')
                    .map(str::trim)
                    .eq(["xmm1", "xmm2", "xmm3"]) =>
        {
            vec![
                Instruction::with3(
                    Code::VEX_Vaddps_xmm_xmm_xmmm128,
                    Register::XMM1,
                    Register::XMM2,
                    Register::XMM3,
                )
                .map_err(|_| "unsupported AVX form")?,
            ]
        }
        _ => return Err("unsupported instruction or operands"),
    };
    let candidates: Vec<_> = instructions
        .iter()
        .filter_map(|instruction| {
            let mut encoder = Encoder::new(bitness);
            encoder.encode(instruction, pc).ok()?;
            Some(encoder.take_buffer())
        })
        .collect();
    if candidates.is_empty() {
        Err("instruction cannot encode at the selected address")
    } else {
        Ok(candidates)
    }
}

fn assemble_aarch64(mnemonic: &str, operands: &str, pc: u64) -> Result<u32, &'static str> {
    if !pc.is_multiple_of(4) {
        return Err("AArch64 instruction address is not aligned");
    }
    match mnemonic {
        "nop" if operands.is_empty() => Ok(0xD503_201F),
        "b" => {
            let target = number(operands)?;
            let displacement = i128::from(target) - i128::from(pc);
            if displacement % 4 != 0 || !(-(1 << 27)..(1 << 27)).contains(&displacement) {
                return Err("AArch64 branch target is misaligned or out of range");
            }
            Ok(0x1400_0000 | ((displacement / 4) as u32 & 0x03FF_FFFF))
        }
        _ => Err("unsupported AArch64 form"),
    }
}

#[derive(Clone, Copy)]
pub struct Case {
    pub profile: Profile,
    pub pc: u64,
    pub input: &'static str,
    pub expected: &'static [&'static [u8]],
}

pub const CASES: &[Case] = &[
    Case {
        profile: Profile::X86_16,
        pc: 0x1000,
        input: "mov ax, 10t",
        expected: &[&[0xB8, 0x0A, 0]],
    },
    Case {
        profile: Profile::X86_32,
        pc: 0x1000,
        input: "mov eax, 12345678",
        expected: &[&[0xB8, 0x78, 0x56, 0x34, 0x12]],
    },
    Case {
        profile: Profile::X86_64,
        pc: 0x1000,
        input: "jmp 1010",
        expected: &[&[0xEB, 0x0E], &[0xE9, 0x0B, 0, 0, 0]],
    },
    Case {
        profile: Profile::X86_64,
        pc: 0x1001,
        input: "jmp 1010",
        expected: &[&[0xEB, 0x0D], &[0xE9, 0x0A, 0, 0, 0]],
    },
    Case {
        profile: Profile::X86_64,
        pc: 0x1000,
        input: "vaddps xmm1,xmm2,xmm3",
        expected: &[&[0xC5, 0xE8, 0x58, 0xCB]],
    },
    Case {
        profile: Profile::Aarch64,
        pc: 0x1000,
        input: "b 1010",
        expected: &[&[0x04, 0, 0, 0x14]],
    },
    Case {
        profile: Profile::Aarch64,
        pc: 0x1004,
        input: "b 1010",
        expected: &[&[0x03, 0, 0, 0x14]],
    },
    Case {
        profile: Profile::Aarch64,
        pc: 0x1000,
        input: "nop",
        expected: &[&[0x1F, 0x20, 0x03, 0xD5]],
    },
    Case {
        profile: Profile::X86_64,
        pc: 0x1000,
        input: "jmp nowhere",
        expected: &[],
    },
    Case {
        profile: Profile::X86_64,
        pc: 0x1000,
        input: "jmp short 2000",
        expected: &[],
    },
    Case {
        profile: Profile::Aarch64,
        pc: 0x1000,
        input: "b 8001000",
        expected: &[],
    },
    Case {
        profile: Profile::Aarch64,
        pc: 0x1000,
        input: "b 1001",
        expected: &[],
    },
];

pub fn verify() -> bool {
    let matches = CASES
        .iter()
        .all(|case| match assemble(case.input, case.profile, case.pc) {
            Ok(bytes) => bytes == case.expected,
            Err(_) => case.expected.is_empty(),
        });
    #[cfg(feature = "compare-asm-rs")]
    let matches = matches && verify_aarch64_alternative();
    matches
}

#[cfg(feature = "compare-asm-rs")]
fn verify_aarch64_alternative() -> bool {
    let encode = |pc, destination| {
        asm_rs::assemble_with(
            "b destination",
            asm_rs::Arch::Aarch64,
            pc,
            &[("destination", destination)],
        )
    };
    encode(0x1000, 0x1010).is_ok_and(|bytes| bytes == [0x04, 0, 0, 0x14])
        && encode(0x1004, 0x1010).is_ok_and(|bytes| bytes == [0x03, 0, 0, 0x14])
        && encode(0x1000, 0x8001000).is_err()
}

#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn verify_assembler_probe() -> u32 {
    u32::from(verify())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn golden_assembly_cases() {
        for case in CASES {
            let result = assemble(case.input, case.profile, case.pc);
            if case.expected.is_empty() {
                assert!(result.is_err(), "{}", case.input);
            } else {
                assert_eq!(result.unwrap(), case.expected, "{}", case.input);
            }
        }
        assert!(assemble("nop\nnop", Profile::X86_64, 0).is_err());
        assert!(assemble(&" ".repeat(129), Profile::X86_64, 0).is_err());
        assert!(verify());
    }
}
