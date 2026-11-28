//! Shared, bounded interpretation state. No terminal or host-CPU assumptions.

use std::collections::VecDeque;

use crate::formats::{self, Endian, Executable, Machine, VirtualAddress};
use crate::instructions::{DecodeMode, DecodedInstruction, decode_one};
use crate::symbols::{self, SymbolTable};
use crate::{ByteSource, Document, Error, FileOffset};

/// Maximum instructions requested in one viewport/navigation operation.
pub const MAX_CODE_ROWS: usize = 256;
/// Maximum retained branch-return and forward-walk positions.
pub const MAX_NAVIGATION_HISTORY: usize = 256;

/// The default HIEW direct-branch shortcuts in their display order.
pub const BRANCH_LABELS: &[u8] = b"123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";

#[derive(Clone, Copy, Debug)]
pub(crate) struct CodeWindow {
    pub start: FileOffset,
    pub limit: usize,
    pub group_nops: bool,
    pub generation: u64,
    pub selected_row: Option<(FileOffset, u64)>,
}

/// Effective decoder selection, kept distinct from a file's declared machine.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DecodeContext {
    pub mode: Option<DecodeMode>,
    pub endian: Endian,
    pub base: VirtualAddress,
    pub overridden: bool,
    pub raw: bool,
}

/// A decoded row begins at a complete-file offset, including a fat-file prefix.
#[derive(Clone, Debug)]
pub struct CodeRow {
    pub offset: FileOffset,
    pub instruction: DecodedInstruction,
    /// True when the instruction address is a mapped or explicitly based VA.
    /// False marks raw file-offset interpretation of an unmapped image region.
    pub mapped: bool,
    pub branch_number: Option<usize>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Interpretation {
    pub mode: Option<DecodeMode>,
    pub endian: Option<Endian>,
    pub base: Option<VirtualAddress>,
    pub raw: bool,
    pub slice: Option<usize>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Location {
    pub offset: FileOffset,
    pub interpretation: Interpretation,
    pub symbol_mode: Option<DecodeMode>,
    pub generation: u64,
}

#[derive(Debug, Default)]
pub(crate) struct Analysis {
    pub interpretation: Interpretation,
    /// A navigation hint from parsed symbols, below an explicit user override.
    pub symbol_mode: Option<DecodeMode>,
    pub executable: Option<Executable>,
    pub problem: Option<String>,
    pub symbols: Option<SymbolTable>,
    pub symbol_problem: Option<String>,
    symbols_loaded: bool,
    generation: Option<u64>,
    pub numbered_window: Option<CodeWindow>,
    pub history: VecDeque<Location>,
    pub walk: VecDeque<(FileOffset, FileOffset)>,
}

impl Analysis {
    pub fn invalidate(&mut self) {
        self.symbol_mode = None;
        self.generation = None;
        self.numbered_window = None;
        self.executable = None;
        self.problem = None;
        self.clear_symbols();
        self.walk.clear();
    }

    pub fn refresh<S: ByteSource>(&mut self, document: &mut Document<S>) {
        if document.generation() != u64::MAX && self.generation == Some(document.generation()) {
            return;
        }
        self.symbol_mode = None;
        self.executable = None;
        self.problem = None;
        self.clear_symbols();
        self.walk.clear();
        if !self.interpretation.raw {
            match formats::inspect(document, self.interpretation.slice) {
                Ok(executable) => self.executable = executable,
                Err(error) => self.problem = Some(error.to_string()),
            }
        }
        self.generation = Some(document.generation());
    }

    fn clear_symbols(&mut self) {
        self.symbols = None;
        self.symbol_problem = None;
        self.symbols_loaded = false;
    }

    pub fn refresh_symbols<S: ByteSource>(&mut self, document: &mut Document<S>) {
        self.refresh(document);
        if self.symbols_loaded && self.is_current(document.generation()) {
            return;
        }
        self.clear_symbols();
        self.symbols_loaded = true;
        let Some(image) = self.executable.as_ref() else {
            self.symbol_problem = Some(self.problem.clone().unwrap_or_else(|| {
                "no executable symbol tables are available in raw interpretation".into()
            }));
            return;
        };
        match symbols::inspect(document, image) {
            Ok(symbols) => self.symbols = Some(symbols),
            Err(error) => self.symbol_problem = Some(error.to_string()),
        }
    }

    pub fn is_current(&self, generation: u64) -> bool {
        generation != u64::MAX && self.generation == Some(generation)
    }

    pub fn context_at(&self, generation: u64) -> DecodeContext {
        if self.is_current(generation) {
            self.context()
        } else {
            Self {
                interpretation: self.interpretation,
                ..Self::default()
            }
            .context()
        }
    }

    pub fn automatic_raw(&self) -> bool {
        self.generation.is_some()
            && !self.interpretation.raw
            && self.problem.is_none()
            && self.executable.is_none()
    }

    pub fn context(&self) -> DecodeContext {
        let parsed = self.executable.as_ref();
        let inferred = parsed.and_then(|image| match image.machine {
            Machine::X86 => Some(DecodeMode::X86_32),
            Machine::X86_64 => Some(DecodeMode::X86_64),
            Machine::Arm => Some(if image.entry.is_some_and(|entry| entry.get() & 1 != 0) {
                DecodeMode::Thumb
            } else {
                DecodeMode::Arm
            }),
            Machine::Aarch64 => Some(DecodeMode::Aarch64),
            Machine::Thumb => Some(DecodeMode::Thumb),
            Machine::Other(_) => None,
        });
        DecodeContext {
            mode: self
                .interpretation
                .mode
                .or(self.symbol_mode)
                .or(inferred)
                .or_else(|| self.automatic_raw().then_some(DecodeMode::X86_32)),
            endian: self
                .interpretation
                .endian
                .or(parsed.map(|image| image.endian))
                .unwrap_or(Endian::Little),
            base: self
                .interpretation
                .base
                .or(parsed.and_then(|image| image.image_base))
                .unwrap_or_default(),
            overridden: self.interpretation.mode.is_some()
                || self.interpretation.endian.is_some()
                || self.interpretation.base.is_some()
                || self.interpretation.raw,
            raw: self.interpretation.raw || parsed.is_none(),
        }
    }

    pub fn address_at(&self, offset: FileOffset) -> Result<(VirtualAddress, bool), Error> {
        if let Some(image) = &self.executable {
            return Ok(match image.file_to_va(offset) {
                Ok(address) => (address, true),
                Err(_) => (VirtualAddress::new(offset.get()), false),
            });
        }
        let address = self
            .interpretation
            .base
            .unwrap_or_default()
            .get()
            .checked_add(offset.get())
            .ok_or_else(|| {
                Error::Analysis("raw base plus file offset overflows a virtual address".into())
            })?;
        Ok((VirtualAddress::new(address), true))
    }

    pub fn virtual_offset(&self, address: VirtualAddress, len: u64) -> Result<FileOffset, Error> {
        let offset = if let Some(image) = &self.executable {
            image.va_to_file(address).map_err(analysis_error)?
        } else {
            if !self.interpretation.raw
                && self.interpretation.mode.is_none()
                && self.interpretation.base.is_none()
            {
                return Err(Error::Analysis(
                    "no virtual mapping; select a raw architecture/base explicitly".into(),
                ));
            }
            let offset = address
                .get()
                .checked_sub(self.context().base.get())
                .ok_or_else(|| {
                    Error::Analysis("virtual address precedes the explicit raw base".into())
                })?;
            FileOffset::new(offset)
        };
        if offset.get() >= len {
            return Err(Error::OutOfBounds {
                offset: offset.get(),
                length: 1,
                document_len: len,
            });
        }
        Ok(offset)
    }

    pub fn branch_offset(&self, address: VirtualAddress, len: u64) -> Result<FileOffset, Error> {
        if !self.automatic_raw() {
            return self.virtual_offset(address, len);
        }
        if address.get() >= len {
            return Err(Error::OutOfBounds {
                offset: address.get(),
                length: 1,
                document_len: len,
            });
        }
        Ok(FileOffset::new(address.get()))
    }

    pub fn entry_offset(&self, len: u64) -> Result<FileOffset, Error> {
        let image = self
            .executable
            .as_ref()
            .ok_or_else(|| Error::Analysis("no executable entry point is available".into()))?;
        let mut entry = image.entry.ok_or_else(|| {
            Error::Analysis("this image declares no supported entry point".into())
        })?;
        // ARM ELF/PE entry pointers may carry the Thumb-state bit. This is not a
        // byte to skip in the file, nor a rule for arbitrary user-entered VAs.
        if matches!(image.machine, Machine::Arm | Machine::Thumb) {
            entry = VirtualAddress::new(entry.get() & !1);
        }
        self.virtual_offset(entry, len)
    }

    pub fn select_slice<S: ByteSource>(
        &mut self,
        document: &mut Document<S>,
        index: usize,
    ) -> Result<FileOffset, Error> {
        let image = formats::inspect(document, Some(index))
            .map_err(analysis_error)?
            .ok_or_else(|| {
                Error::Analysis("the document is not a universal Mach-O container".into())
            })?;
        let offset = image.slice.offset;
        self.interpretation.slice = Some(index);
        self.interpretation.raw = false;
        self.interpretation.base = None;
        self.symbol_mode = None;
        self.executable = Some(image);
        self.problem = None;
        self.clear_symbols();
        self.generation = Some(document.generation());
        self.walk.clear();
        Ok(offset)
    }

    pub fn rows<S: ByteSource>(
        &mut self,
        document: &mut Document<S>,
        start: FileOffset,
        limit: usize,
        group_nops: bool,
    ) -> Result<Vec<CodeRow>, Error> {
        if limit > MAX_CODE_ROWS {
            return Err(Error::LimitExceeded {
                resource: "code viewport rows",
                limit: MAX_CODE_ROWS,
            });
        }
        self.refresh(document);
        if start.get() >= document.len() || limit == 0 {
            return Ok(Vec::new());
        }
        let context = self.context();
        let mode = context.mode.ok_or_else(|| {
            Error::Analysis(
                "select an instruction architecture; no supported ISA was inferred".into(),
            )
        })?;
        let mut rows = Vec::with_capacity(limit);
        let mut offset = start;
        let mut branch_destinations = Vec::with_capacity(BRANCH_LABELS.len());
        for _ in 0..limit {
            let (address, mapped) = self.address_at(offset)?;
            let mut buffer = [0u8; 15];
            let mut read_len = mode.max_instruction_len().min(buffer.len());
            if let Some(image) = &self.executable {
                // Decode only a contiguous VA span. File-adjacent regions can
                // describe unrelated virtual memory or ambiguous mappings.
                for byte in 1..read_len {
                    let contiguous = offset.get().checked_add(byte as u64).is_some_and(|file| {
                        let next = image.file_to_va(FileOffset::new(file));
                        if mapped {
                            address
                                .get()
                                .checked_add(byte as u64)
                                .is_some_and(|va| next == Ok(VirtualAddress::new(va)))
                        } else {
                            next.is_err()
                        }
                    });
                    if !contiguous {
                        read_len = byte;
                        break;
                    }
                }
            }
            let count = document.read_at(offset, &mut buffer[..read_len])?;
            let Some(mut instruction) = decode_one(&buffer[..count], address, mode, context.endian)
            else {
                break;
            };
            if instruction.length == 0 || instruction.length > count {
                return Err(Error::Analysis(
                    "decoder returned a non-progressing or out-of-range instruction".into(),
                ));
            }
            if !mapped {
                instruction.direct_branch = None;
            }
            if group_nops && context.endian == Endian::Little && mode.is_x86() && buffer[0] == 0x90
            {
                let length = buffer[..count]
                    .iter()
                    .take_while(|byte| **byte == 0x90)
                    .count();
                instruction.length = length;
                instruction.bytes[..length].fill(0x90);
            }
            let numbered = instruction.direct_branch.as_ref().and_then(|target| {
                if let Some(index) = branch_destinations.iter().position(|known| known == target) {
                    Some(index + 1)
                } else if branch_destinations.len() < BRANCH_LABELS.len() {
                    branch_destinations.push(*target);
                    Some(branch_destinations.len())
                } else {
                    None
                }
            });
            let next = offset
                .get()
                .checked_add(instruction.length as u64)
                .ok_or_else(|| Error::Analysis("instruction file offset overflow".into()))?;
            rows.push(CodeRow {
                offset,
                instruction,
                mapped,
                branch_number: numbered,
            });
            if next >= document.len() {
                break;
            }
            offset = FileOffset::new(next);
        }
        Ok(rows)
    }

    pub fn remember(&mut self, location: Location) {
        if self.history.len() == MAX_NAVIGATION_HISTORY {
            self.history.pop_front();
        }
        self.history.push_back(location);
    }

    pub fn remember_step(&mut self, from: FileOffset, to: FileOffset) {
        if self.walk.len() == MAX_NAVIGATION_HISTORY {
            self.walk.pop_front();
        }
        self.walk.push_back((from, to));
    }
}

pub(crate) fn analysis_error(error: impl std::fmt::Display) -> Error {
    Error::Analysis(error.to_string())
}
