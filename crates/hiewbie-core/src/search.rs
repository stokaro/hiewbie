use crate::{ByteSource, Document, Error, FileOffset, MAX_READ_BYTES};

/// Maximum literal search pattern size and prefix-table length.
pub const MAX_SEARCH_PATTERN_BYTES: usize = 4096;

/// Result of one bounded search step. Drop a pending search to cancel it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SearchProgress {
    Pending { next: FileOffset },
    Found(FileOffset),
    NotFound,
}

/// Forward, literal, incremental search against one document generation.
///
/// KMP prefix state preserves matches across chunks without rereading their
/// bytes. One step reads at most `min(budget, MAX_READ_BYTES)` bytes and does
/// linear matching work. Cancellation has no document-side state to clean up.
#[derive(Debug)]
pub struct Search {
    pattern: Vec<u8>,
    prefixes: Vec<usize>,
    next: FileOffset,
    matched: usize,
    generation: u64,
    result: Option<SearchProgress>,
}

impl Search {
    pub fn new(pattern: Vec<u8>, start: FileOffset, generation: u64) -> Result<Self, Error> {
        if pattern.is_empty() {
            return Err(Error::InvalidPattern("the pattern is empty"));
        }
        if pattern.len() > MAX_SEARCH_PATTERN_BYTES {
            return Err(Error::LimitExceeded {
                resource: "search pattern bytes",
                limit: MAX_SEARCH_PATTERN_BYTES,
            });
        }
        let mut prefixes = vec![0; pattern.len()];
        let mut matched = 0;
        for index in 1..pattern.len() {
            while matched != 0 && pattern[index] != pattern[matched] {
                matched = prefixes[matched - 1];
            }
            if pattern[index] == pattern[matched] {
                matched += 1;
            }
            prefixes[index] = matched;
        }
        Ok(Self {
            pattern,
            prefixes,
            next: start,
            matched: 0,
            generation,
            result: None,
        })
    }

    pub fn step<S: ByteSource>(
        &mut self,
        document: &mut Document<S>,
        budget: usize,
    ) -> Result<SearchProgress, Error> {
        if document.generation() == u64::MAX {
            return Err(Error::GenerationExhausted);
        }
        if document.generation() != self.generation {
            return Err(Error::StaleSearch {
                expected: self.generation,
                actual: document.generation(),
            });
        }
        if let Some(result) = self.result {
            return Ok(result);
        }
        if self.next.get() > document.len() {
            return Err(Error::OutOfBounds {
                offset: self.next.get(),
                length: 0,
                document_len: document.len(),
            });
        }
        if self.next.get() == document.len() {
            self.result = Some(SearchProgress::NotFound);
            return Ok(SearchProgress::NotFound);
        }
        if budget == 0 {
            return Ok(SearchProgress::Pending { next: self.next });
        }
        let count = budget
            .min(MAX_READ_BYTES)
            .min((document.len() - self.next.get()).min(MAX_READ_BYTES as u64) as usize);
        let mut buffer = vec![0; count];
        // Read before advancing any search state, so an I/O failure is retryable.
        document.read_at(self.next, &mut buffer)?;
        for byte in buffer {
            while self.matched != 0 && byte != self.pattern[self.matched] {
                self.matched = self.prefixes[self.matched - 1];
            }
            if byte == self.pattern[self.matched] {
                self.matched += 1;
            }
            self.next = FileOffset::new(self.next.get() + 1);
            if self.matched == self.pattern.len() {
                let result = SearchProgress::Found(FileOffset::new(
                    self.next.get() - self.pattern.len() as u64,
                ));
                self.result = Some(result);
                return Ok(result);
            }
        }
        if self.next.get() == document.len() {
            self.result = Some(SearchProgress::NotFound);
            Ok(SearchProgress::NotFound)
        } else {
            Ok(SearchProgress::Pending { next: self.next })
        }
    }
}
