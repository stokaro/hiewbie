use crate::analysis::{Analysis, Interpretation};
use crate::formats::{Endian, Executable, VirtualAddress};
use crate::instructions::{InstructionProfile, MAX_INSTRUCTION_BYTES, scan_references};
use crate::{ByteSource, Document, Error, FileOffset, MAX_READ_BYTES, SearchProgress};

const LOOKAHEAD_BYTES: usize = MAX_INSTRUCTION_BYTES - 1;
const MAX_STEP_BYTES: usize = MAX_READ_BYTES - LOOKAHEAD_BYTES;

#[derive(Debug, Default)]
pub(crate) struct ReferenceSearch {
    pub pending: Option<ReferenceScan>,
    pub resume: Option<ReferenceScan>,
    pub bytewise: bool,
}

/// A linear scan over one document and interpretation. The resume position
/// always points to the next candidate, including across read boundaries.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ReferenceScan {
    target: VirtualAddress,
    profile: InstructionProfile,
    interpretation: Interpretation,
    generation: u64,
    next: FileOffset,
    end: u64,
    bytewise: bool,
}

impl ReferenceScan {
    pub(crate) fn new(
        analysis: &Analysis,
        cursor: FileOffset,
        len: u64,
        generation: u64,
        bytewise: bool,
    ) -> Result<Self, Error> {
        let context = analysis.context();
        let mode = context
            .mode
            .filter(|mode| mode.is_x86())
            .ok_or_else(|| Error::Analysis("select an x86 profile to find references".into()))?;
        if context.endian != Endian::Little {
            return Err(Error::Analysis(
                "x86 reference search requires little-endian instruction bytes".into(),
            ));
        }
        let (target, mapped) = analysis.address_at(cursor)?;
        if !mapped
            || len == 0
            || analysis
                .executable
                .as_ref()
                .is_some_and(|image| image.va_to_file(target) != Ok(cursor))
        {
            return Err(Error::Analysis(
                "the current location has no reference address".into(),
            ));
        }
        let (next, end) = match &analysis.executable {
            Some(image) => (
                image.slice.offset,
                image
                    .slice
                    .end()
                    .map_err(crate::analysis::analysis_error)?
                    .get()
                    .min(len),
            ),
            None => (FileOffset::new(0), len),
        };
        Ok(Self {
            target,
            profile: analysis.instruction_profile(mode),
            interpretation: analysis.interpretation,
            generation,
            next,
            end,
            bytewise,
        })
    }

    pub(crate) fn is_current(&self, analysis: &Analysis, generation: u64) -> bool {
        self.generation == generation
            && self.interpretation == analysis.interpretation
            && analysis
                .context()
                .mode
                .is_some_and(|mode| analysis.instruction_profile(mode) == self.profile)
    }

    pub(crate) const fn position(&self) -> FileOffset {
        self.next
    }

    pub(crate) fn step<S: ByteSource>(
        &mut self,
        analysis: &Analysis,
        document: &mut Document<S>,
        budget: usize,
    ) -> Result<SearchProgress, Error> {
        if self.next.get() >= self.end {
            return Ok(SearchProgress::NotFound);
        }
        if budget == 0 {
            return Ok(SearchProgress::Pending { next: self.next });
        }
        let (address, mapped) = analysis.address_at(self.next)?;
        let span_end = analysis.executable.as_ref().map_or(self.end, |image| {
            mapping_run_end(image, self.next.get(), self.end)
        });
        if !mapped {
            self.next = FileOffset::new(span_end);
        } else {
            let available = span_end - self.next.get();
            let limit = available.min(budget.min(MAX_STEP_BYTES) as u64) as usize;
            let read_len = available.min((limit + LOOKAHEAD_BYTES) as u64) as usize;
            let mut buffer = [0; MAX_READ_BYTES];
            let count = document.read_at(self.next, &mut buffer[..read_len])?;
            let start = self.next.get();
            let (advanced, hit) = scan_references(
                &buffer[..count],
                limit.min(count),
                address,
                self.profile,
                self.target,
                analysis.executable.is_some(),
                self.bytewise,
            );
            self.next = FileOffset::new(start + advanced as u64);
            if let Some(hit) = hit {
                return Ok(SearchProgress::Found(FileOffset::new(start + hit as u64)));
            }
        }
        if self.next.get() >= self.end {
            Ok(SearchProgress::NotFound)
        } else {
            Ok(SearchProgress::Pending { next: self.next })
        }
    }
}

// Mapping can change at any region boundary, including ambiguity in an overlap.
// Cap a read before that point so an instruction never crosses unrelated spans.
fn mapping_run_end(image: &Executable, offset: u64, mut end: u64) -> u64 {
    for region in &image.regions {
        let start = region.file.offset.get();
        let size = region.file.len.min(region.memory_size);
        if size == 0 {
            continue;
        }
        if start > offset {
            end = end.min(start);
        } else if let Some(region_end) = start.checked_add(size)
            && region_end > offset
        {
            end = end.min(region_end);
        }
    }
    end
}
