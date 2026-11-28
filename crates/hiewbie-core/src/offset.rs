use crate::{Error, formats::VirtualAddress};

/// An absolute offset in the raw file, independent of virtual-address mappings.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FileOffset(u64);

impl FileOffset {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Parse a checked raw-file goto expression.
///
/// Numbers are hexadecimal unless suffixed with `t` (decimal). A leading `+`
/// or `-` is relative to `current`; optional `0x` is accepted for hexadecimal.
/// Dotted virtual addresses are explicitly unavailable until a mapping backend
/// exists. Empty files have the sole valid position zero; other positions must
/// identify an existing byte rather than the position after EOF.
pub fn parse_goto(input: &str, current: FileOffset, len: u64) -> Result<FileOffset, Error> {
    let input = input.trim();
    if input.contains('.') {
        return Err(Error::UnsupportedAddress);
    }
    let (relative, value) = match input.as_bytes().first() {
        Some(b'+') => (Some(true), &input[1..]),
        Some(b'-') => (Some(false), &input[1..]),
        _ => (None, input),
    };
    let value = parse_number(value)?;
    let offset = match relative {
        Some(true) => current.get().checked_add(value),
        Some(false) => current.get().checked_sub(value),
        None => Some(value),
    }
    .ok_or(Error::InvalidGoto("relative offset overflow or underflow"))?;
    if (len == 0 && offset != 0) || (len != 0 && offset >= len) {
        return Err(Error::OutOfBounds {
            offset,
            length: 1,
            document_len: len,
        });
    }
    Ok(FileOffset::new(offset))
}

/// Parse an absolute virtual address without imposing file-length constraints.
/// A leading dot is optional here; the shared goto command uses it to distinguish
/// this domain from file offsets. Relative expressions are deliberately rejected.
pub fn parse_virtual_address(input: &str) -> Result<VirtualAddress, Error> {
    let input = input.trim();
    parse_number(input.strip_prefix('.').unwrap_or(input)).map(VirtualAddress::new)
}

fn parse_number(value: &str) -> Result<u64, Error> {
    let (radix, digits) = if let Some(digits) = value.strip_suffix(['t', 'T']) {
        (10, digits)
    } else {
        (
            16,
            value
                .strip_prefix("0x")
                .or_else(|| value.strip_prefix("0X"))
                .unwrap_or(value),
        )
    };
    if digits.is_empty()
        || !digits.bytes().all(|byte| match radix {
            10 => byte.is_ascii_digit(),
            _ => byte.is_ascii_hexdigit(),
        })
    {
        return Err(Error::InvalidGoto(
            "expected hexadecimal digits or decimal digits followed by t",
        ));
    }
    u64::from_str_radix(digits, radix)
        .map_err(|_| Error::InvalidGoto("the number does not fit in a 64-bit address"))
}
