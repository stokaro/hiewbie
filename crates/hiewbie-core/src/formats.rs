//! Bounded executable metadata and address translation over logical document bytes.
//!
//! These independently written readers cover image headers and load mappings, not
//! a platform loader or full symbol/import/relocation interpretation. Failure
//! never changes the document; callers retain its raw-byte view.

use std::{error, fmt};

use crate::{ByteSource, Document, Error, FileOffset};

mod elf;
mod macho;
mod pe;

#[cfg(test)]
mod tests;

/// Maximum bytes read by one inspection, independent of document length.
pub const MAX_FORMAT_READ_BYTES: usize = 1024 * 1024;
/// Maximum entries in an image's section/program/load-command table.
pub const MAX_FORMAT_ENTRIES: usize = 4096;
/// Maximum architectures in a universal Mach-O container.
pub const MAX_FAT_SLICES: usize = 64;
/// Maximum interpreted numeric fields retained by one inspection.
pub const MAX_FORMAT_FIELDS: usize = 32768;

/// Byte order declared by the inspected image, independent of the host.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Endian {
    Little,
    Big,
}

impl fmt::Display for Endian {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Little => "little",
            Self::Big => "big",
        })
    }
}

macro_rules! address_type {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(u64);
        impl $name {
            #[must_use]
            pub const fn new(value: u64) -> Self {
                Self(value)
            }
            #[must_use]
            pub const fn get(self) -> u64 {
                self.0
            }
        }
    };
}

address_type!(
    VirtualAddress,
    "An image virtual address, not a raw-file offset."
);
address_type!(
    Rva,
    "An address relative to an image's explicitly declared base."
);
address_type!(
    SliceOffset,
    "An offset relative to the selected image/container slice."
);

/// A half-open range in the complete raw file, including a container's prefix.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileRange {
    pub offset: FileOffset,
    pub len: u64,
}

impl FileRange {
    #[must_use]
    pub fn contains(self, offset: FileOffset) -> bool {
        offset
            .get()
            .checked_sub(self.offset.get())
            .is_some_and(|value| value < self.len)
    }

    pub fn end(self) -> Result<FileOffset, AddressError> {
        self.offset
            .get()
            .checked_add(self.len)
            .map(FileOffset::new)
            .ok_or(AddressError::Overflow)
    }
}

/// The image's machine declaration, not a selected decoder or host CPU.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Machine {
    X86,
    X86_64,
    Arm,
    /// A format machine tag that explicitly requires Thumb/Thumb-2 instructions.
    Thumb,
    Aarch64,
    Other(u32),
}

impl fmt::Display for Machine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::X86 => f.write_str("x86"),
            Self::X86_64 => f.write_str("x86-64"),
            Self::Arm => f.write_str("ARM"),
            Self::Thumb => f.write_str("ARM/Thumb"),
            Self::Aarch64 => f.write_str("AArch64"),
            Self::Other(value) => write!(f, "machine 0x{value:X}"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BinaryFormat {
    Pe32,
    Pe64,
    Elf32,
    Elf64,
    MachO32,
    MachO64,
}

impl fmt::Display for BinaryFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Pe32 => "PE32",
            Self::Pe64 => "PE32+",
            Self::Elf32 => "ELF32",
            Self::Elf64 => "ELF64",
            Self::MachO32 => "Mach-O32",
            Self::MachO64 => "Mach-O64",
        })
    }
}

/// One load mapping. Disk padding beyond `memory_size` remains raw/unmapped.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Region {
    pub name: String,
    pub file: FileRange,
    pub address: VirtualAddress,
    pub memory_size: u64,
    pub readable: bool,
    pub writable: bool,
    pub executable: bool,
    pub header: FileRange,
}

/// An interpreted numeric field linked to its exact backing bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HeaderField {
    pub name: String,
    pub value: u64,
    pub range: FileRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FatSlice {
    pub index: usize,
    pub machine: Machine,
    pub cpu_type: u32,
    pub cpu_subtype: u32,
    pub file: FileRange,
}

/// Owned metadata for one document generation and one selected image.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Executable {
    pub generation: u64,
    pub format: BinaryFormat,
    pub machine: Machine,
    pub endian: Endian,
    pub bitness: u8,
    pub image_base: Option<VirtualAddress>,
    pub entry: Option<VirtualAddress>,
    pub slice: FileRange,
    pub selected_slice: Option<usize>,
    pub slices: Vec<FatSlice>,
    pub regions: Vec<Region>,
    pub fields: Vec<HeaderField>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AddressError {
    Unmapped,
    ZeroFill,
    Ambiguous,
    Overflow,
    NoImageBase,
}

impl fmt::Display for AddressError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Unmapped => "address has no file-backed mapping",
            Self::ZeroFill => "address is zero-fill memory and has no file bytes",
            Self::Ambiguous => "address belongs to overlapping mappings",
            Self::Overflow => "address arithmetic overflow",
            Self::NoImageBase => "this format declares no RVA image base",
        })
    }
}

impl error::Error for AddressError {}

impl Executable {
    pub fn file_to_va(&self, offset: FileOffset) -> Result<VirtualAddress, AddressError> {
        let mut result = None;
        for region in &self.regions {
            if !region.file.contains(offset) {
                continue;
            }
            let delta = offset.get() - region.file.offset.get();
            if delta >= region.memory_size {
                continue;
            }
            let address = region
                .address
                .get()
                .checked_add(delta)
                .ok_or(AddressError::Overflow)?;
            if result.replace(VirtualAddress::new(address)).is_some() {
                return Err(AddressError::Ambiguous);
            }
        }
        result.ok_or(AddressError::Unmapped)
    }

    pub fn va_to_file(&self, address: VirtualAddress) -> Result<FileOffset, AddressError> {
        let mut result = None;
        for region in &self.regions {
            let Some(delta) = address.get().checked_sub(region.address.get()) else {
                continue;
            };
            if delta >= region.memory_size {
                continue;
            }
            let mapped = if delta >= region.file.len {
                Err(AddressError::ZeroFill)
            } else {
                region
                    .file
                    .offset
                    .get()
                    .checked_add(delta)
                    .map(FileOffset::new)
                    .ok_or(AddressError::Overflow)
            };
            if result.replace(mapped).is_some() {
                return Err(AddressError::Ambiguous);
            }
        }
        result.unwrap_or(Err(AddressError::Unmapped))
    }

    pub fn rva_to_va(&self, rva: Rva) -> Result<VirtualAddress, AddressError> {
        self.image_base
            .ok_or(AddressError::NoImageBase)?
            .get()
            .checked_add(rva.get())
            .map(VirtualAddress::new)
            .ok_or(AddressError::Overflow)
    }

    pub fn va_to_rva(&self, address: VirtualAddress) -> Result<Rva, AddressError> {
        address
            .get()
            .checked_sub(self.image_base.ok_or(AddressError::NoImageBase)?.get())
            .map(Rva::new)
            .ok_or(AddressError::Unmapped)
    }

    pub fn slice_to_file(&self, offset: SliceOffset) -> Result<FileOffset, AddressError> {
        if offset.get() >= self.slice.len {
            return Err(AddressError::Unmapped);
        }
        self.slice
            .offset
            .get()
            .checked_add(offset.get())
            .map(FileOffset::new)
            .ok_or(AddressError::Overflow)
    }

    pub fn file_to_slice(&self, offset: FileOffset) -> Result<SliceOffset, AddressError> {
        if !self.slice.contains(offset) {
            return Err(AddressError::Unmapped);
        }
        Ok(SliceOffset::new(offset.get() - self.slice.offset.get()))
    }
}

/// An inspection failure never changes logical document bytes or edit state.
#[derive(Debug)]
pub enum FormatError {
    Document(Error),
    Invalid {
        message: &'static str,
        offset: FileOffset,
    },
    Unsupported(&'static str),
    Limit {
        resource: &'static str,
        limit: usize,
    },
    InvalidSlice {
        requested: usize,
        count: usize,
    },
}

impl fmt::Display for FormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Document(error) => write!(f, "format inspection: {error}"),
            Self::Invalid { message, offset } => {
                write!(f, "{message} at file offset 0x{:X}", offset.get())
            }
            Self::Unsupported(feature) => {
                write!(f, "format inspection does not yet support {feature}")
            }
            Self::Limit { resource, limit } => write!(f, "format {resource} exceeds limit {limit}"),
            Self::InvalidSlice { requested, count } => {
                write!(f, "slice {requested} is unavailable ({count} slices)")
            }
        }
    }
}

impl error::Error for FormatError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            Self::Document(error) => Some(error),
            _ => None,
        }
    }
}

impl From<Error> for FormatError {
    fn from(error: Error) -> Self {
        Self::Document(error)
    }
}

/// Inspect headers and mappings without reading the complete file.
///
/// `None` means unrecognized raw data. Recognized malformed/unsupported data
/// returns an error so the caller can explain the raw-view fallback. Universal
/// Mach-O defaults to the first table entry, never the host architecture; the
/// selected index and all validated slice descriptors remain visible.
pub fn inspect<S: ByteSource>(
    document: &mut Document<S>,
    slice: Option<usize>,
) -> Result<Option<Executable>, FormatError> {
    let generation = document.generation();
    let len = document.len();
    let mut reader = Reader {
        document,
        slice: FileRange {
            offset: FileOffset::new(0),
            len,
        },
        read_bytes: 0,
    };
    let magic = reader.magic()?;
    if macho::is_fat(magic) {
        return macho::parse_fat(&mut reader, slice, generation).map(Some);
    }
    if let Some(requested) = slice {
        return Err(FormatError::InvalidSlice {
            requested,
            count: 0,
        });
    }
    parse_image(&mut reader, magic, generation)
}

fn parse_image<S: ByteSource>(
    reader: &mut Reader<'_, S>,
    magic: [u8; 4],
    generation: u64,
) -> Result<Option<Executable>, FormatError> {
    if magic.starts_with(b"MZ") {
        pe::parse(reader, generation).map(Some)
    } else if magic == *b"\x7fELF" {
        elf::parse(reader, generation).map(Some)
    } else if macho::is_thin(magic) {
        macho::parse(reader, magic, generation).map(Some)
    } else {
        Ok(None)
    }
}

struct Reader<'a, S: ByteSource> {
    document: &'a mut Document<S>,
    slice: FileRange,
    read_bytes: usize,
}

impl<S: ByteSource> Reader<'_, S> {
    fn magic(&mut self) -> Result<[u8; 4], FormatError> {
        if self.slice.len >= 4 {
            return self.read::<4>(0);
        }
        let mut bytes = [0; 4];
        let count = self.slice.len.min(4) as usize;
        self.read_bytes = self
            .read_bytes
            .checked_add(count)
            .filter(|value| *value <= MAX_FORMAT_READ_BYTES)
            .ok_or(FormatError::Limit {
                resource: "read bytes",
                limit: MAX_FORMAT_READ_BYTES,
            })?;
        self.document
            .read_at(self.slice.offset, &mut bytes[..count])?;
        Ok(bytes)
    }

    fn invalid(&self, message: &'static str, offset: u64) -> FormatError {
        FormatError::Invalid {
            message,
            offset: FileOffset::new(self.slice.offset.get().saturating_add(offset)),
        }
    }

    fn range(&self, offset: u64, len: u64) -> Result<FileRange, FormatError> {
        if offset > self.slice.len || len > self.slice.len - offset {
            return Err(self.invalid("truncated or out-of-range structure", offset));
        }
        let absolute = self
            .slice
            .offset
            .get()
            .checked_add(offset)
            .ok_or_else(|| self.invalid("file offset overflow", offset))?;
        Ok(FileRange {
            offset: FileOffset::new(absolute),
            len,
        })
    }

    fn read<const N: usize>(&mut self, offset: u64) -> Result<[u8; N], FormatError> {
        let range = self.range(offset, N as u64)?;
        self.read_bytes = self
            .read_bytes
            .checked_add(N)
            .filter(|count| *count <= MAX_FORMAT_READ_BYTES)
            .ok_or(FormatError::Limit {
                resource: "read bytes",
                limit: MAX_FORMAT_READ_BYTES,
            })?;
        let mut out = [0; N];
        if self.document.read_at(range.offset, &mut out)? != N {
            return Err(self.invalid("short structure read", offset));
        }
        Ok(out)
    }

    fn table(&self, offset: u64, count: usize, size: u64) -> Result<(), FormatError> {
        if count > MAX_FORMAT_ENTRIES {
            return Err(FormatError::Limit {
                resource: "table entries",
                limit: MAX_FORMAT_ENTRIES,
            });
        }
        let len = (count as u64)
            .checked_mul(size)
            .ok_or_else(|| self.invalid("table length overflow", offset))?;
        self.range(offset, len)?;
        Ok(())
    }

    fn field(
        &self,
        image: &mut Executable,
        name: impl Into<String>,
        value: u64,
        offset: u64,
        len: u64,
    ) -> Result<(), FormatError> {
        if image.fields.len() >= MAX_FORMAT_FIELDS {
            return Err(FormatError::Limit {
                resource: "header fields",
                limit: MAX_FORMAT_FIELDS,
            });
        }
        image.fields.push(HeaderField {
            name: name.into(),
            value,
            range: self.range(offset, len)?,
        });
        Ok(())
    }

    fn region(&self, image: &mut Executable, region: Region) -> Result<(), FormatError> {
        region
            .address
            .get()
            .checked_add(region.memory_size)
            .ok_or_else(|| {
                self.invalid(
                    "virtual range overflow",
                    region
                        .header
                        .offset
                        .get()
                        .saturating_sub(self.slice.offset.get()),
                )
            })?;
        image.regions.push(region);
        Ok(())
    }
}

fn image<S: ByteSource>(
    reader: &Reader<'_, S>,
    generation: u64,
    format: BinaryFormat,
    machine: Machine,
    endian: Endian,
    bitness: u8,
) -> Executable {
    Executable {
        generation,
        format,
        machine,
        endian,
        bitness,
        image_base: None,
        entry: None,
        slice: reader.slice,
        selected_slice: None,
        slices: Vec::new(),
        regions: Vec::new(),
        fields: Vec::new(),
    }
}

fn u16_at(bytes: &[u8], offset: usize, endian: Endian) -> u16 {
    let value = [bytes[offset], bytes[offset + 1]];
    match endian {
        Endian::Little => u16::from_le_bytes(value),
        Endian::Big => u16::from_be_bytes(value),
    }
}

fn u32_at(bytes: &[u8], offset: usize, endian: Endian) -> u32 {
    let value = [
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ];
    match endian {
        Endian::Little => u32::from_le_bytes(value),
        Endian::Big => u32::from_be_bytes(value),
    }
}

fn u64_at(bytes: &[u8], offset: usize, endian: Endian) -> u64 {
    let value = [
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
        bytes[offset + 4],
        bytes[offset + 5],
        bytes[offset + 6],
        bytes[offset + 7],
    ];
    match endian {
        Endian::Little => u64::from_le_bytes(value),
        Endian::Big => u64::from_be_bytes(value),
    }
}

fn name(bytes: &[u8]) -> String {
    bytes
        .iter()
        .take_while(|byte| **byte != 0)
        .map(|byte| {
            if byte.is_ascii_graphic() || *byte == b' ' {
                char::from(*byte)
            } else {
                '.'
            }
        })
        .collect()
}
