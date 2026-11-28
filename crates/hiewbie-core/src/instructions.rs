//! Bounded, host-independent instruction decoding with explicit interpretation.
//!
//! A successful result means that the selected backend recognized the bytes.
//! It does not establish architectural validity, full extension coverage, or
//! execution safety. Every nonempty input makes progress, including unknown
//! encodings and truncated tails. No inspected instruction is executed.

use std::fmt::{self, Write};
use std::str::FromStr;

use crate::formats::{Endian, VirtualAddress};

mod arm;
mod x86;

pub(crate) use x86::scan_references;

#[cfg(test)]
mod tests;

pub const MAX_INSTRUCTION_BYTES: usize = 15;
pub const MAX_INSTRUCTION_TEXT_BYTES: usize = 256;

/// The instruction interpretation, independent of the host and parsed machine.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DecodeMode {
    X86_16,
    X86_32,
    X86_64,
    Arm,
    Thumb,
    Aarch64,
}

impl DecodeMode {
    pub const fn is_x86(self) -> bool {
        matches!(self, Self::X86_16 | Self::X86_32 | Self::X86_64)
    }

    pub const fn max_instruction_len(self) -> usize {
        match self {
            Self::X86_16 | Self::X86_32 | Self::X86_64 => MAX_INSTRUCTION_BYTES,
            Self::Arm | Self::Thumb | Self::Aarch64 => 4,
        }
    }

    /// Smallest unit consumed when the backend cannot interpret an encoding.
    pub const fn alignment(self) -> usize {
        match self {
            Self::X86_16 | Self::X86_32 | Self::X86_64 => 1,
            Self::Thumb => 2,
            Self::Arm | Self::Aarch64 => 4,
        }
    }

    pub const fn bitness(self) -> u32 {
        match self {
            Self::X86_16 => 16,
            Self::X86_32 | Self::Arm | Self::Thumb => 32,
            Self::X86_64 | Self::Aarch64 => 64,
        }
    }

    pub const fn backend(self) -> &'static str {
        match self {
            Self::X86_16 | Self::X86_32 | Self::X86_64 => "iced-x86 1.22.0",
            Self::Arm | Self::Thumb | Self::Aarch64 => "yaxpeax-arm 0.5.0",
        }
    }
}

impl fmt::Display for DecodeMode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::X86_16 => "x86-16",
            Self::X86_32 => "x86-32",
            Self::X86_64 => "x86-64",
            Self::Arm => "arm",
            Self::Thumb => "thumb",
            Self::Aarch64 => "aarch64",
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParseDecodeModeError;

impl fmt::Display for ParseDecodeModeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("expected x86-16, x86-32, x86-64, arm, thumb, or aarch64")
    }
}

impl std::error::Error for ParseDecodeModeError {}

impl FromStr for DecodeMode {
    type Err = ParseDecodeModeError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "x86-16" | "x86_16" => Ok(Self::X86_16),
            "x86-32" | "x86_32" | "x86" => Ok(Self::X86_32),
            "x86-64" | "x86_64" | "x64" => Ok(Self::X86_64),
            "arm" => Ok(Self::Arm),
            "thumb" => Ok(Self::Thumb),
            "aarch64" | "arm64" => Ok(Self::Aarch64),
            _ => Err(ParseDecodeModeError),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DecodeStatus {
    Decoded,
    /// Rejected or unknown to this backend; not a definitive ISA-validity claim.
    Unknown,
    /// A recognized encoding is unavailable in the selected instruction mode.
    InvalidForMode,
    Truncated,
    Unsupported,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BranchTarget {
    pub address: VirtualAddress,
    pub mode: DecodeMode,
}

/// An address encoded by an x86 operand. A memory-indirect control transfer
/// encodes a pointer slot whose contents require a separate mapping check.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AddressOperand {
    Memory(VirtualAddress),
    Literal(VirtualAddress),
    Indirect { slot: VirtualAddress, width: usize },
}

impl AddressOperand {
    #[must_use]
    pub fn address(self) -> VirtualAddress {
        match self {
            Self::Memory(address) | Self::Literal(address) => address,
            Self::Indirect { slot, .. } => slot,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecodedInstruction {
    pub address: VirtualAddress,
    pub length: usize,
    pub bytes: [u8; MAX_INSTRUCTION_BYTES],
    pub text: String,
    pub status: DecodeStatus,
    /// Direct near branches only. Indirect/far destinations remain unresolved.
    pub direct_branch: Option<BranchTarget>,
    pub address_operand: Option<AddressOperand>,
}

impl DecodedInstruction {
    pub fn bytes(&self) -> &[u8] {
        &self.bytes[..self.length.min(MAX_INSTRUCTION_BYTES)]
    }
}

/// Decode at most one instruction. Empty input returns `None`.
///
/// Big-endian instruction interpretations are explicitly unsupported in this
/// first adapter: ARM BE8/BE32 and container-data endianness are distinct issues.
/// x86 always uses little-endian instructions. Input beyond the architecture's
/// maximum instruction length is not passed to a backend.
pub fn decode_one(
    bytes: &[u8],
    address: VirtualAddress,
    mode: DecodeMode,
    endian: Endian,
) -> Option<DecodedInstruction> {
    if bytes.is_empty() {
        return None;
    }
    let bytes = &bytes[..bytes.len().min(mode.max_instruction_len())];
    Some(if endian != Endian::Little {
        fallback(bytes, address, mode, DecodeStatus::Unsupported)
    } else {
        match mode {
            DecodeMode::X86_16 | DecodeMode::X86_32 | DecodeMode::X86_64 => {
                x86::decode(bytes, address, mode)
            }
            DecodeMode::Arm | DecodeMode::Thumb => arm::decode_arm(bytes, address, mode),
            DecodeMode::Aarch64 => arm::decode_aarch64(bytes, address),
        }
    })
}

fn instruction(
    bytes: &[u8],
    address: VirtualAddress,
    text: String,
    status: DecodeStatus,
    direct_branch: Option<BranchTarget>,
) -> DecodedInstruction {
    let length = bytes.len().min(MAX_INSTRUCTION_BYTES);
    let mut backing = [0; MAX_INSTRUCTION_BYTES];
    backing[..length].copy_from_slice(&bytes[..length]);
    DecodedInstruction {
        address,
        length,
        bytes: backing,
        text,
        status,
        direct_branch,
        address_operand: None,
    }
}

fn fallback(
    bytes: &[u8],
    address: VirtualAddress,
    mode: DecodeMode,
    status: DecodeStatus,
) -> DecodedInstruction {
    let bytes = &bytes[..bytes.len().min(mode.alignment())];
    let mut text = TextBuffer::new();
    text.append("db ");
    for (index, byte) in bytes.iter().enumerate() {
        if index != 0 {
            text.append(", ");
        }
        let _ = write!(text, "0x{byte:02x}");
    }
    text.append(match status {
        DecodeStatus::Decoded => "",
        DecodeStatus::Unknown => " ; unknown to backend",
        DecodeStatus::InvalidForMode => " ; invalid in this instruction mode",
        DecodeStatus::Truncated => " ; truncated instruction",
        DecodeStatus::Unsupported => " ; unsupported instruction endianness",
    });
    instruction(bytes, address, text.finish(), status, None)
}

/// Formatter output is bounded independently of instruction-byte storage.
struct TextBuffer {
    text: String,
    clipped: bool,
}

impl TextBuffer {
    fn new() -> Self {
        Self {
            text: String::with_capacity(MAX_INSTRUCTION_TEXT_BYTES),
            clipped: false,
        }
    }

    fn append(&mut self, text: &str) {
        let available = MAX_INSTRUCTION_TEXT_BYTES - self.text.len();
        let mut length = available.min(text.len());
        while !text.is_char_boundary(length) {
            length -= 1;
        }
        self.text.push_str(&text[..length]);
        self.clipped |= length < text.len();
    }

    fn finish(mut self) -> String {
        if self.clipped {
            while self.text.len() > MAX_INSTRUCTION_TEXT_BYTES - 3 {
                self.text.pop();
            }
            self.text.push_str("...");
        }
        self.text
    }
}

impl fmt::Write for TextBuffer {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        self.append(text);
        Ok(())
    }
}

fn relative_target(
    address: VirtualAddress,
    displacement: i64,
    mode: DecodeMode,
) -> Option<BranchTarget> {
    address
        .get()
        .checked_add_signed(displacement)
        .map(|value| BranchTarget {
            address: VirtualAddress::new(value),
            mode,
        })
}
