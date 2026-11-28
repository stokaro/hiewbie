use std::sync::Arc;

use super::{ByteSource, Editor, Error, FileRange, ViewMode, VirtualAddress};
use crate::analysis::{DecodeContext, Interpretation, analysis_error};
use crate::document::MAX_EDIT_BYTES;
use crate::instructions::DecodeMode;

/// The maximum replacement span selected by the caller.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AssemblySpan {
    CurrentInstruction,
    Explicit(usize),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AssemblyPadding {
    Preserve,
    Nop,
}

/// Reviewed bytes bound to one edit session, position, profile, and generation.
#[derive(Debug)]
pub struct AssemblyPatch {
    session: Arc<()>,
    generation: u64,
    context: DecodeContext,
    interpretation: Interpretation,
    address: VirtualAddress,
    range: FileRange,
    original: Vec<u8>,
    replacement: Vec<u8>,
}

impl AssemblyPatch {
    #[must_use]
    pub const fn address(&self) -> VirtualAddress {
        self.address
    }

    #[must_use]
    pub const fn range(&self) -> FileRange {
        self.range
    }

    /// Current logical bytes of the entire reviewed span.
    #[must_use]
    pub fn original(&self) -> &[u8] {
        &self.original
    }

    /// Bytes to write starting at the beginning of the reviewed span.
    /// The remaining original bytes, if any, are preserved.
    #[must_use]
    pub fn replacement(&self) -> &[u8] {
        &self.replacement
    }
}

impl<S: ByteSource> Editor<S> {
    /// Review one candidate at the current Code cursor without editing bytes.
    /// The index uses the ordering returned by `assemble_at_cursor`.
    pub fn preview_assembly_patch(
        &mut self,
        source: &str,
        candidate: usize,
        span: AssemblySpan,
        padding: AssemblyPadding,
    ) -> Result<AssemblyPatch, Error> {
        self.require_code_edit()?;
        let row = self
            .analysis
            .rows(&mut self.document, self.cursor, 1, false)?
            .pop()
            .filter(|row| row.mapped)
            .ok_or_else(|| analysis_error("Current instruction has no file-backed address"))?;
        let bytes = self
            .assemble_at_cursor(source)?
            .into_iter()
            .nth(candidate)
            .ok_or_else(|| analysis_error("No such assembly candidate"))?;
        let current_len = row.instruction.length;
        let span = match span {
            AssemblySpan::CurrentInstruction => current_len,
            AssemblySpan::Explicit(span) if span >= current_len => span,
            AssemblySpan::Explicit(_) => {
                return Err(analysis_error(
                    "Explicit assembly span is smaller than the current instruction",
                ));
            }
        };
        if bytes.len() > span {
            return Err(analysis_error(format!(
                "Encoding needs {} bytes. The selected span allows {span}.",
                bytes.len(),
            )));
        }
        if span > MAX_EDIT_BYTES {
            return Err(Error::LimitExceeded {
                resource: "assembly patch span",
                limit: MAX_EDIT_BYTES,
            });
        }

        let context = self.decode_context();
        let range = FileRange {
            offset: self.cursor,
            len: span as u64,
        };
        let address = row.instruction.address;
        self.check_assembly_range(range, address)?;
        let mut original = vec![0; span];
        let read = self.document.read_at(range.offset, &mut original)?;
        if read != span {
            return Err(analysis_error("Assembly span extends beyond the file"));
        }
        let mut replacement = bytes;
        if padding == AssemblyPadding::Nop {
            match context.mode {
                Some(mode) if mode.is_x86() => replacement.resize(span, 0x90),
                Some(DecodeMode::Aarch64) if span.is_multiple_of(4) => {
                    while replacement.len() < span {
                        replacement.extend_from_slice(&[0x1f, 0x20, 0x03, 0xd5]);
                    }
                }
                Some(DecodeMode::Aarch64) => {
                    return Err(analysis_error(
                        "AArch64 NOP padding requires a four-byte-aligned span",
                    ));
                }
                _ => return Err(analysis_error("NOP padding is unavailable in this profile")),
            }
        }
        Ok(AssemblyPatch {
            session: Arc::clone(self.edit_session.as_ref().ok_or(Error::NotEditing)?),
            generation: self.document.generation(),
            context,
            interpretation: self.analysis.interpretation,
            address,
            range,
            original,
            replacement,
        })
    }

    /// Commit the reviewed replacement as one undoable overwrite and advance
    /// by the number of written bytes.
    pub fn apply_assembly_patch(&mut self, patch: &AssemblyPatch) -> Result<(), Error> {
        self.require_code_edit()?;
        self.refresh_analysis();
        let session = self.edit_session.as_ref().ok_or(Error::NotEditing)?;
        if !Arc::ptr_eq(session, &patch.session)
            || self.cursor != patch.range.offset
            || self.document.generation() != patch.generation
            || self.decode_context() != patch.context
            || self.analysis.interpretation != patch.interpretation
        {
            return Err(analysis_error("Assembly preview is stale. Assemble again."));
        }
        self.check_assembly_range(patch.range, patch.address)?;
        let mut current = vec![0; patch.original.len()];
        if self.document.read_at(patch.range.offset, &mut current)? != current.len()
            || current != patch.original
        {
            return Err(analysis_error("Assembly preview is stale. Assemble again."));
        }
        self.document
            .overwrite(patch.range.offset, &patch.replacement)?;
        self.cursor = self.clamp(patch.range.offset.get() + patch.replacement.len() as u64);
        Ok(())
    }

    fn require_code_edit(&self) -> Result<(), Error> {
        self.require_editing()?;
        if self.mode != ViewMode::Code {
            return Err(analysis_error("Assembly is available in Code edit mode"));
        }
        Ok(())
    }

    fn check_assembly_range(&self, range: FileRange, address: VirtualAddress) -> Result<(), Error> {
        let start = range.offset.get();
        let end = range.end().map_err(analysis_error)?.get();
        if end > self.document.len() {
            return Err(analysis_error("Assembly span extends beyond the file"));
        }
        let address = address.get();
        address
            .checked_add(range.len - 1)
            .ok_or_else(|| analysis_error("Assembly span overflows virtual addresses"))?;

        let disjoint =
            || analysis_error("Assembly span lacks a contiguous, unambiguous virtual mapping");
        let Some(image) = &self.analysis.executable else {
            return if self.analysis.address_at(range.offset)?
                == (VirtualAddress::new(address), true)
            {
                Ok(())
            } else {
                Err(disjoint())
            };
        };

        let mut mapped = Vec::new();
        for region in &image.regions {
            let base = region.file.offset.get();
            let region_end = base
                .checked_add(region.file.len.min(region.memory_size))
                .ok_or_else(disjoint)?;
            let first = start.max(base);
            let stop = end.min(region_end);
            if first < stop {
                let virtual_first = region.address.get().checked_add(first - base);
                if virtual_first != Some(address + (first - start)) {
                    return Err(disjoint());
                }
                mapped.push((first, stop));
            }
        }
        mapped.sort_unstable();
        let mut next = start;
        for (first, stop) in mapped {
            if first != next {
                return Err(disjoint());
            }
            next = stop;
        }
        if next != end {
            return Err(disjoint());
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "assembly_patch_tests.rs"]
mod tests;
