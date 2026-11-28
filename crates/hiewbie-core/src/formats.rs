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
mod te;

#[cfg(test)]
mod macho_tests;
#[cfg(test)]
mod te_tests;
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
/// Maximum metadata records retained across all tables in one image.
pub const MAX_FORMAT_RECORDS: usize = 4096;
/// Maximum explanatory notes retained by one inspection.
pub const MAX_FORMAT_NOTES: usize = 256;
/// Maximum bytes in each retained record name, detail, or inspection note.
pub const MAX_FORMAT_TEXT_BYTES: usize = 1024;

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
    Te32,
    Te64,
    /// The common TE header with a machine whose instruction width is unknown.
    TeUnknown,
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
            Self::Te32 => "TE",
            Self::Te64 => "TE64",
            Self::TeUnknown => "TE (unknown machine width)",
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

/// ARM execution state explicitly supplied by entry-point metadata.
/// It does not change the format's declared machine or select a host backend.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EntryMode {
    Arm,
    Thumb,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StructureKind {
    LoadCommand,
    Section,
    Relocation,
    DebugDirectory,
    DebugData,
}

impl fmt::Display for StructureKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::LoadCommand => "load command",
            Self::Section => "section",
            Self::Relocation => "relocation",
            Self::DebugDirectory => "debug directory",
            Self::DebugData => "debug data",
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StructureStatus {
    Interpreted,
    Unsupported,
}

impl fmt::Display for StructureStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Interpreted => "interpreted",
            Self::Unsupported => "unsupported",
        })
    }
}

/// A bounded metadata record with separate source and checked destination spans.
/// An unsupported interpretation never prevents following its raw record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StructureRecord {
    pub name: String,
    pub kind: StructureKind,
    pub record: FileRange,
    pub target: Option<FileRange>,
    pub status: StructureStatus,
    pub detail: String,
}

/// A selection is valid only for the logical generation and container member
/// that supplied its index. Frontends must not reuse it after edits or switching.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StructureRef {
    pub generation: u64,
    pub slice: FileRange,
    pub selected_slice: Option<usize>,
    pub index: usize,
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
    pub entry_mode: Option<EntryMode>,
    pub slice: FileRange,
    pub selected_slice: Option<usize>,
    pub slices: Vec<FatSlice>,
    pub regions: Vec<Region>,
    pub fields: Vec<HeaderField>,
    pub records: Vec<StructureRecord>,
    pub notes: Vec<String>,
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
    #[must_use]
    pub fn structure_ref(&self, index: usize) -> Option<StructureRef> {
        self.records.get(index)?;
        Some(StructureRef {
            generation: self.generation,
            slice: self.slice,
            selected_slice: self.selected_slice,
            index,
        })
    }

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
    } else if magic.starts_with(b"VZ") {
        te::parse(reader, generation).map(Some)
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
        if image.regions.len() >= MAX_FORMAT_ENTRIES {
            return Err(FormatError::Limit {
                resource: "load regions",
                limit: MAX_FORMAT_ENTRIES,
            });
        }
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

    fn record(&self, image: &mut Executable, record: StructureRecord) -> Result<(), FormatError> {
        if image.records.len() >= MAX_FORMAT_RECORDS {
            return Err(FormatError::Limit {
                resource: "structure records",
                limit: MAX_FORMAT_RECORDS,
            });
        }
        for text in [&record.name, &record.detail] {
            self.check_text(text)?;
        }
        for range in [Some(record.record), record.target].into_iter().flatten() {
            let offset = range
                .offset
                .get()
                .checked_sub(self.slice.offset.get())
                .ok_or_else(|| self.invalid("structure span precedes selected image", 0))?;
            self.range(offset, range.len)?;
        }
        image.records.push(record);
        Ok(())
    }

    fn check_text(&self, text: &str) -> Result<(), FormatError> {
        if text.len() > MAX_FORMAT_TEXT_BYTES {
            return Err(FormatError::Limit {
                resource: "structure text bytes",
                limit: MAX_FORMAT_TEXT_BYTES,
            });
        }
        Ok(())
    }

    fn note(&self, image: &mut Executable, note: impl Into<String>) -> Result<(), FormatError> {
        if image.notes.len() >= MAX_FORMAT_NOTES {
            return Err(FormatError::Limit {
                resource: "inspection notes",
                limit: MAX_FORMAT_NOTES,
            });
        }
        let note = note.into();
        self.check_text(&note)?;
        image.notes.push(note);
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
        entry_mode: None,
        slice: reader.slice,
        selected_slice: None,
        slices: Vec::new(),
        regions: Vec::new(),
        fields: Vec::new(),
        records: Vec::new(),
        notes: Vec::new(),
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
