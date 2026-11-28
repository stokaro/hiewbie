use std::ops::Range;

use crate::{ByteSource, Document, Error, FileOffset, MAX_READ_BYTES};

/// Maximum search pattern size and prefix-table length.
pub const MAX_SEARCH_PATTERN_BYTES: usize = 4096;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SearchDirection {
    #[default]
    Forward,
    Backward,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SearchCase {
    #[default]
    Exact,
    AsciiInsensitive,
}

/// A validated byte or UTF-8 query. UTF-8 case folding affects ASCII bytes only.
#[derive(Clone, Debug, Eq, PartialEq)]
enum SearchPattern {
    Bytes(Vec<u8>),
    Utf8(String, SearchCase),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchRequest {
    pattern: SearchPattern,
    pub direction: SearchDirection,
    /// Half-open file byte range. A match must fit entirely inside it.
    pub scope: Option<Range<FileOffset>>,
    /// Require an even absolute file offset at the start of each hit.
    pub even: bool,
}

impl SearchRequest {
    pub fn bytes(bytes: Vec<u8>) -> Result<Self, Error> {
        Self::new(SearchPattern::Bytes(bytes))
    }

    pub fn utf8(text: String, case: SearchCase) -> Result<Self, Error> {
        Self::new(SearchPattern::Utf8(text, case))
    }

    fn new(pattern: SearchPattern) -> Result<Self, Error> {
        let len = match &pattern {
            SearchPattern::Bytes(bytes) => bytes.len(),
            SearchPattern::Utf8(text, _) => text.len(),
        };
        if len == 0 {
            return Err(Error::InvalidPattern("the pattern is empty"));
        }
        if len > MAX_SEARCH_PATTERN_BYTES {
            return Err(Error::LimitExceeded {
                resource: "search pattern bytes",
                limit: MAX_SEARCH_PATTERN_BYTES,
            });
        }
        Ok(Self {
            pattern,
            direction: SearchDirection::Forward,
            scope: None,
            even: false,
        })
    }
}

/// Result of one bounded search step. Drop a pending search to cancel it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SearchProgress {
    Pending { next: FileOffset },
    Found(FileOffset),
    NotFound,
}

/// Incremental search against one document generation.
///
/// KMP prefix state preserves matches across chunks in either direction.
/// Each step reads at most `min(budget, MAX_READ_BYTES)` bytes and does
/// linear matching work.
#[derive(Debug)]
pub struct Search {
    pattern: Vec<u8>,
    prefixes: Vec<usize>,
    case: SearchCase,
    direction: SearchDirection,
    scope: Option<Range<FileOffset>>,
    even: bool,
    start: FileOffset,
    next: Option<u64>,
    matched: usize,
    generation: u64,
    result: Option<SearchProgress>,
}

impl Search {
    pub fn new(pattern: Vec<u8>, start: FileOffset, generation: u64) -> Result<Self, Error> {
        Ok(Self::from_request(
            &SearchRequest::bytes(pattern)?,
            start,
            generation,
        ))
    }

    /// Search from an inclusive candidate start offset in the requested direction.
    #[must_use]
    pub fn from_request(request: &SearchRequest, start: FileOffset, generation: u64) -> Self {
        let (mut pattern, case) = match &request.pattern {
            SearchPattern::Bytes(bytes) => (bytes.clone(), SearchCase::Exact),
            SearchPattern::Utf8(text, case) => (text.as_bytes().to_vec(), *case),
        };
        if case == SearchCase::AsciiInsensitive {
            pattern.make_ascii_lowercase();
        }
        if request.direction == SearchDirection::Backward {
            pattern.reverse();
        }
        let mut prefixes = vec![0; pattern.len()];
        let mut matched = 0;
        for index in 1..pattern.len() {
            matched = prefix_match(&pattern, &prefixes, matched, pattern[index]);
            prefixes[index] = matched;
        }
        Self {
            pattern,
            prefixes,
            case,
            direction: request.direction,
            scope: request.scope.clone(),
            even: request.even,
            start,
            next: None,
            matched: 0,
            generation,
            result: None,
        }
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

        let (first, end) = self.scope.as_ref().map_or((0, document.len()), |scope| {
            (scope.start.get(), scope.end.get())
        });
        if first > end {
            return Err(Error::InvalidPattern("search scope is reversed"));
        }
        for offset in [end, self.start.get()] {
            if offset > document.len() {
                return Err(Error::OutOfBounds {
                    offset,
                    length: 0,
                    document_len: document.len(),
                });
            }
        }

        let width = self.pattern.len() as u64;
        let forward = self.direction == SearchDirection::Forward;
        // Backward scanning consumes bytes in reverse order, starting after
        // the rightmost candidate's final byte. A hit's offset is still its
        // first byte, regardless of traversal direction.
        let mut next = self.next.unwrap_or(if forward {
            self.start.get().clamp(first, end)
        } else if self.start.get() < first {
            first
        } else {
            self.start.get().saturating_add(width).min(end)
        });
        let boundary = if forward { end } else { first };
        let remaining = next.abs_diff(boundary);
        if remaining < width - self.matched as u64 {
            return Ok(self.finish(SearchProgress::NotFound));
        }

        let count = remaining.min(budget.min(MAX_READ_BYTES) as u64) as usize;
        if count == 0 {
            return Ok(SearchProgress::Pending {
                next: FileOffset::new(next),
            });
        }
        let mut buffer = vec![0; count];
        let read_start = if forward { next } else { next - count as u64 };
        // Read before advancing any search state, so an I/O failure is retryable.
        document.read_at(FileOffset::new(read_start), &mut buffer)?;
        if !forward {
            buffer.reverse();
        }
        if self.case == SearchCase::AsciiInsensitive {
            buffer.make_ascii_lowercase();
        }
        for byte in buffer {
            self.matched = prefix_match(&self.pattern, &self.prefixes, self.matched, byte);
            next = if forward { next + 1 } else { next - 1 };
            if self.matched == self.pattern.len() {
                let hit = if forward { next - width } else { next };
                if !self.even || hit % 2 == 0 {
                    return Ok(self.finish(SearchProgress::Found(FileOffset::new(hit))));
                }
                self.matched = self.prefixes[self.matched - 1];
            }
        }

        self.next = Some(next);
        if next == boundary {
            Ok(self.finish(SearchProgress::NotFound))
        } else {
            Ok(SearchProgress::Pending {
                next: FileOffset::new(next),
            })
        }
    }

    fn finish(&mut self, result: SearchProgress) -> SearchProgress {
        self.result = Some(result);
        result
    }
}

fn prefix_match(pattern: &[u8], prefixes: &[usize], mut matched: usize, byte: u8) -> usize {
    while matched != 0 && byte != pattern[matched] {
        matched = prefixes[matched - 1];
    }
    matched + usize::from(byte == pattern[matched])
}

/// Completed query and pending scan owned by the document editor. A canceled
/// or failed scan leaves the previously completed query and its resume point.
#[derive(Debug, Default)]
pub(crate) struct SearchSession {
    completed: Option<CompletedSearch>,
    pending: Option<(SearchRequest, Search)>,
    last_hit: Option<(FileOffset, u64)>,
}

#[derive(Debug)]
struct CompletedSearch {
    request: SearchRequest,
    next_start: Option<FileOffset>,
    generation: u64,
}

impl SearchSession {
    pub(crate) fn last_hit(&self, generation: u64) -> Option<FileOffset> {
        self.last_hit
            .filter(|(_, hit_generation)| *hit_generation == generation)
            .map(|(offset, _)| offset)
    }

    pub(crate) fn is_pending(&self) -> bool {
        self.pending.is_some()
    }

    pub(crate) fn cancel(&mut self) {
        self.pending = None;
    }

    pub(crate) fn reset(&mut self, cursor: FileOffset, generation: u64) {
        if let Some(completed) = &mut self.completed {
            completed.next_start = Some(cursor);
            completed.generation = generation;
        }
    }

    pub(crate) fn start(&mut self, request: SearchRequest, cursor: FileOffset, generation: u64) {
        let search = Search::from_request(&request, cursor, generation);
        self.pending = Some((request, search));
    }

    pub(crate) fn repeat(&mut self, cursor: FileOffset, generation: u64, len: u64) -> SearchRepeat {
        let Some(completed) = &self.completed else {
            return SearchRepeat::NoQuery;
        };
        let next_start = if completed.generation == generation {
            completed.next_start
        } else {
            Some(cursor)
        };
        let Some(start) = next_start.filter(|offset| offset.get() < len) else {
            return SearchRepeat::Exhausted;
        };
        self.start(completed.request.clone(), start, generation);
        SearchRepeat::Started
    }

    pub(crate) fn step<S: ByteSource>(
        &mut self,
        document: &mut Document<S>,
        budget: usize,
    ) -> Result<Option<SearchProgress>, Error> {
        let Some((request, mut search)) = self.pending.take() else {
            return Ok(None);
        };
        let progress = search.step(document, budget)?;
        if let SearchProgress::Found(hit) = progress {
            self.last_hit = Some((hit, document.generation()));
        }
        match progress {
            SearchProgress::Pending { .. } => self.pending = Some((request, search)),
            SearchProgress::Found(_) | SearchProgress::NotFound => {
                let next_start = match (progress, request.direction) {
                    (SearchProgress::Found(hit), SearchDirection::Forward) => {
                        hit.get().checked_add(1).map(FileOffset::new)
                    }
                    (SearchProgress::Found(hit), SearchDirection::Backward) => {
                        hit.get().checked_sub(1).map(FileOffset::new)
                    }
                    _ => None,
                };
                self.completed = Some(CompletedSearch {
                    request,
                    next_start,
                    generation: document.generation(),
                });
            }
        }
        Ok(Some(progress))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SearchRepeat {
    Started,
    NoQuery,
    Exhausted,
}
