//! Pure textual assembly for an explicitly selected inspected address and mode.

use std::fmt;

use crate::formats::{Endian, VirtualAddress};
use crate::instructions::DecodeMode;

mod syntax;
mod x86;

#[cfg(test)]
mod tests;

/// Maximum source bytes for a single ASCII assembly instruction.
pub const MAX_SOURCE_BYTES: usize = 256;

/// A textual instruction could not be assembled.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AssemblyError {
    UnsupportedProfile,
    SourceLimit,
    Syntax(&'static str),
    UnknownMnemonic(String),
    UnresolvedSymbol(String),
    AmbiguousMemorySize,
    UnsupportedForm(&'static str),
    OutOfRange(&'static str),
}

impl fmt::Display for AssemblyError {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedProfile => output.write_str("Select a little-endian x86 profile"),
            Self::SourceLimit => write!(output, "Assembly input exceeds {MAX_SOURCE_BYTES} bytes"),
            Self::Syntax(reason) | Self::UnsupportedForm(reason) | Self::OutOfRange(reason) => {
                output.write_str(reason)
            }
            Self::UnknownMnemonic(name) => write!(output, "Illegal instruction: {name}"),
            Self::UnresolvedSymbol(name) => {
                write!(output, "Unresolved or ambiguous symbol: {name}")
            }
            Self::AmbiguousMemorySize => output.write_str("Specify the memory operand size"),
        }
    }
}

impl std::error::Error for AssemblyError {}

/// Assemble one textual x86 instruction and return distinct byte encodings.
/// Alternatives are ordered by length and then by their bytes.
/// JMP uses a near branch and JMPS selects its short form.
/// The resolver maps case-sensitive names to inspected virtual addresses.
pub fn assemble_x86(
    source: &str,
    mode: DecodeMode,
    endian: Endian,
    address: VirtualAddress,
    resolve: impl FnMut(&str) -> Option<VirtualAddress>,
) -> Result<Vec<Vec<u8>>, AssemblyError> {
    if !mode.is_x86() || endian != Endian::Little {
        return Err(AssemblyError::UnsupportedProfile);
    }
    if source.len() > MAX_SOURCE_BYTES {
        return Err(AssemblyError::SourceLimit);
    }
    let instruction = syntax::parse(source, resolve)?;
    x86::encode(&instruction, mode, address)
}
