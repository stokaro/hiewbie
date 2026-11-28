//! On-demand, bounded symbol/import/export metadata over logical document bytes.
//!
//! This is separate from opening an executable or rendering Code. Table failures
//! leave the existing image mappings and raw-byte editing available.

use crate::formats::{
    AddressError, BinaryFormat, Endian, Executable, FileRange, FormatError, Rva, VirtualAddress,
};
use crate::{ByteSource, Document, FileOffset, MAX_READ_BYTES};

mod elf;
mod macho;
mod pe;

#[cfg(test)]
mod tests;

pub const MAX_SYMBOLS: usize = 16_384;
pub const MAX_SYMBOL_READ_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_SYMBOL_NAME_BYTES: usize = 1024;
pub const MAX_SYMBOL_TEXT_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_SYMBOL_NOTES: usize = 32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SymbolKind {
    Defined,
    Import,
    Export,
}

impl std::fmt::Display for SymbolKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Defined => "symbol",
            Self::Import => "import",
            Self::Export => "export",
        })
    }
}

/// A named definition, a pointer slot, and an unresolved name are distinct.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SymbolTarget {
    Virtual(VirtualAddress),
    /// An ARM-state function whose symbol metadata establishes its ISA.
    Arm(VirtualAddress),
    /// An ARM Thumb function pointer; the low bit is state, not a file byte.
    Thumb(VirtualAddress),
    File(FileOffset),
    ImportSlot(VirtualAddress),
    Unavailable,
}

impl SymbolTarget {
    pub fn file_offset(self, image: &Executable) -> Result<FileOffset, AddressError> {
        let offset = match self {
            Self::Virtual(address) | Self::Arm(address) | Self::ImportSlot(address) => {
                image.va_to_file(address)?
            }
            Self::Thumb(address) => image.va_to_file(VirtualAddress::new(address.get() & !1))?,
            Self::File(offset) => offset,
            Self::Unavailable => return Err(AddressError::Unmapped),
        };
        if image.slice.contains(offset) {
            Ok(offset)
        } else {
            Err(AddressError::Unmapped)
        }
    }
}

/// Interpretation only: following `record` exposes metadata, never rebuilds it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub library: Option<String>,
    pub ordinal: Option<u64>,
    pub target: SymbolTarget,
    pub record: FileRange,
    pub size: Option<u64>,
    pub detail: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SymbolTable {
    pub generation: u64,
    pub slice: FileRange,
    pub entries: Vec<Symbol>,
    /// Coverage limitations, including loader metadata that is not interpreted.
    pub notes: Vec<String>,
}

impl SymbolTable {
    fn new(image: &Executable) -> Self {
        Self {
            generation: image.generation,
            slice: image.slice,
            entries: Vec::new(),
            notes: Vec::new(),
        }
    }
}

/// Parse only on explicit request. The image must describe the same logical bytes.
pub fn inspect<S: ByteSource>(
    document: &mut Document<S>,
    image: &Executable,
) -> Result<SymbolTable, FormatError> {
    if image.generation == u64::MAX || image.generation != document.generation() {
        return Err(FormatError::Unsupported(
            "symbols from stale executable metadata",
        ));
    }
    let mut reader = Reader {
        document,
        slice: image.slice,
        read_bytes: 0,
        text_bytes: 0,
    };
    reader.range(0, image.slice.len)?;
    match image.format {
        BinaryFormat::Pe32 | BinaryFormat::Pe64 => pe::parse(&mut reader, image),
        BinaryFormat::Elf32 | BinaryFormat::Elf64 => elf::parse(&mut reader, image),
        BinaryFormat::MachO32 | BinaryFormat::MachO64 => macho::parse(&mut reader, image),
        BinaryFormat::Te32 | BinaryFormat::Te64 | BinaryFormat::TeUnknown => {
            Err(FormatError::Unsupported("TE symbol/import/export tables"))
        }
    }
}

struct Reader<'a, S: ByteSource> {
    document: &'a mut Document<S>,
    slice: FileRange,
    read_bytes: usize,
    text_bytes: usize,
}

impl<S: ByteSource> Reader<'_, S> {
    /// `offset` is relative to the selected image, including for fat Mach-O.
    fn invalid(&self, message: &'static str, offset: u64) -> FormatError {
        FormatError::Invalid {
            message,
            offset: FileOffset::new(self.slice.offset.get().saturating_add(offset)),
        }
    }

    fn range(&self, offset: u64, len: u64) -> Result<FileRange, FormatError> {
        let absolute = self
            .slice
            .offset
            .get()
            .checked_add(offset)
            .ok_or_else(|| self.invalid("symbol file offset overflow", offset))?;
        if offset > self.slice.len
            || len > self.slice.len - offset
            || absolute > self.document.len()
            || len > self.document.len() - absolute
        {
            return Err(self.invalid(
                "symbol structure is outside the selected file slice",
                offset,
            ));
        }
        Ok(FileRange {
            offset: FileOffset::new(absolute),
            len,
        })
    }

    fn charge_read(&mut self, length: usize) -> Result<(), FormatError> {
        self.read_bytes = self
            .read_bytes
            .checked_add(length)
            .filter(|total| *total <= MAX_SYMBOL_READ_BYTES)
            .ok_or(FormatError::Limit {
                resource: "symbol read bytes",
                limit: MAX_SYMBOL_READ_BYTES,
            })?;
        Ok(())
    }

    fn read<const N: usize>(&mut self, offset: u64) -> Result<[u8; N], FormatError> {
        let range = self.range(offset, N as u64)?;
        self.charge_read(N)?;
        let mut bytes = [0; N];
        if self.document.read_at(range.offset, &mut bytes)? != N {
            return Err(self.invalid("short symbol structure read", offset));
        }
        Ok(bytes)
    }

    fn bytes(&mut self, offset: u64, length: usize) -> Result<Vec<u8>, FormatError> {
        let range = self.range(offset, length as u64)?;
        self.charge_read(length)?;
        let mut bytes = vec![0; length];
        for (index, chunk) in bytes.chunks_mut(MAX_READ_BYTES).enumerate() {
            let at = range.offset.get() + (index * MAX_READ_BYTES) as u64;
            if self.document.read_at(FileOffset::new(at), chunk)? != chunk.len() {
                return Err(self.invalid("short symbol data read", offset));
            }
        }
        Ok(bytes)
    }

    fn table(&self, offset: u64, count: u64, size: u64) -> Result<(), FormatError> {
        if count > MAX_SYMBOLS as u64 {
            return Err(FormatError::Limit {
                resource: "symbol table entries",
                limit: MAX_SYMBOLS,
            });
        }
        if count != 0 && size == 0 {
            return Err(self.invalid("zero-size symbol table entry", offset));
        }
        let length = count
            .checked_mul(size)
            .ok_or_else(|| self.invalid("symbol table size overflow", offset))?;
        self.range(offset, length)?;
        Ok(())
    }

    /// Validate the entire VA span, including overlaps in its interior.
    fn va(
        &self,
        image: &Executable,
        address: VirtualAddress,
        length: u64,
    ) -> Result<u64, FormatError> {
        let end = address
            .get()
            .checked_add(length.max(1))
            .ok_or_else(|| self.invalid("symbol virtual span overflow", 0))?;
        let mut owner = None;
        for region in &image.regions {
            let region_end = region
                .address
                .get()
                .checked_add(region.memory_size)
                .ok_or_else(|| self.invalid("symbol mapping overflow", 0))?;
            if region.memory_size != 0
                && address.get() < region_end
                && end > region.address.get()
                && owner.replace(region).is_some()
            {
                return Err(self.invalid("symbol span crosses or overlaps image mappings", 0));
            }
        }
        let region = owner.ok_or_else(|| self.invalid("symbol address is unmapped", 0))?;
        let backed_end = region
            .address
            .get()
            .checked_add(region.memory_size.min(region.file.len))
            .ok_or_else(|| self.invalid("symbol mapping overflow", 0))?;
        if address.get() < region.address.get() || end > backed_end {
            return Err(self.invalid("symbol span has no contiguous file bytes", 0));
        }
        let file = region
            .file
            .offset
            .get()
            .checked_add(address.get() - region.address.get())
            .ok_or_else(|| self.invalid("symbol file offset overflow", 0))?;
        let relative = file
            .checked_sub(self.slice.offset.get())
            .ok_or_else(|| self.invalid("symbol precedes selected slice", 0))?;
        self.range(relative, length)?;
        Ok(relative)
    }

    fn rva(&self, image: &Executable, rva: u64, length: u64) -> Result<u64, FormatError> {
        let address = image
            .rva_to_va(Rva::new(rva))
            .map_err(|_| self.invalid("symbol RVA overflow or unavailable image base", 0))?;
        self.va(image, address, length)
    }

    /// Read a NUL-terminated name within an explicit slice-relative boundary.
    fn c_string(&mut self, offset: u64, end: u64) -> Result<String, FormatError> {
        if end < offset {
            return Err(self.invalid("reversed symbol string range", offset));
        }
        self.range(offset, end - offset)?;
        let mut bytes = Vec::new();
        let mut at = offset;
        while at < end && bytes.len() <= MAX_SYMBOL_NAME_BYTES {
            let count = (end - at)
                .min(64)
                .min((MAX_SYMBOL_NAME_BYTES + 1 - bytes.len()) as u64)
                as usize;
            let chunk = self.bytes(at, count)?;
            if let Some(nul) = chunk.iter().position(|byte| *byte == 0) {
                bytes.extend_from_slice(&chunk[..nul]);
                return Ok(String::from_utf8_lossy(&bytes).into_owned());
            }
            bytes.extend_from_slice(&chunk);
            at += count as u64;
        }
        if bytes.len() > MAX_SYMBOL_NAME_BYTES {
            Err(FormatError::Limit {
                resource: "symbol name bytes",
                limit: MAX_SYMBOL_NAME_BYTES,
            })
        } else {
            Err(self.invalid("unterminated symbol name", offset))
        }
    }

    fn retain_text(&mut self, length: usize) -> Result<(), FormatError> {
        self.text_bytes = self
            .text_bytes
            .checked_add(length)
            .filter(|total| *total <= MAX_SYMBOL_TEXT_BYTES)
            .ok_or(FormatError::Limit {
                resource: "symbol text bytes",
                limit: MAX_SYMBOL_TEXT_BYTES,
            })?;
        Ok(())
    }

    fn push(&mut self, table: &mut SymbolTable, symbol: Symbol) -> Result<(), FormatError> {
        if table.entries.len() >= MAX_SYMBOLS {
            return Err(FormatError::Limit {
                resource: "symbols",
                limit: MAX_SYMBOLS,
            });
        }
        let relative = symbol
            .record
            .offset
            .get()
            .checked_sub(self.slice.offset.get())
            .ok_or_else(|| self.invalid("symbol record precedes selected slice", 0))?;
        if symbol.record.len == 0 {
            return Err(self.invalid("empty symbol record", relative));
        }
        self.range(relative, symbol.record.len)?;
        if let SymbolTarget::File(offset) = symbol.target {
            let relative = offset
                .get()
                .checked_sub(self.slice.offset.get())
                .ok_or_else(|| self.invalid("symbol target precedes selected slice", 0))?;
            self.range(relative, 1)?;
        }
        self.retain_text(symbol.name.len())?;
        self.retain_text(symbol.detail.len())?;
        self.retain_text(symbol.library.as_ref().map_or(0, String::len))?;
        table.entries.push(symbol);
        Ok(())
    }

    fn note(&mut self, table: &mut SymbolTable, note: &str) -> Result<(), FormatError> {
        if table.notes.iter().any(|existing| existing == note) {
            return Ok(());
        }
        if table.notes.len() >= MAX_SYMBOL_NOTES {
            return Err(FormatError::Limit {
                resource: "symbol coverage notes",
                limit: MAX_SYMBOL_NOTES,
            });
        }
        self.retain_text(note.len())?;
        table.notes.push(note.to_owned());
        Ok(())
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
    let mut value = [0; 8];
    value.copy_from_slice(&bytes[offset..offset + 8]);
    match endian {
        Endian::Little => u64::from_le_bytes(value),
        Endian::Big => u64::from_be_bytes(value),
    }
}
