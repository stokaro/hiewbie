use std::{collections::BTreeMap, io, mem::size_of};

use crate::{Error, FileOffset};

/// Maximum bytes requested from a source by one document read.
pub const MAX_READ_BYTES: usize = 64 * 1024;
/// Maximum span changed by one overwrite command.
pub const MAX_EDIT_BYTES: usize = 64 * 1024;
/// Maximum distinct bytes in the pending sparse overlay.
pub const MAX_PENDING_BYTES: usize = 1024 * 1024;
/// Maximum combined undo/redo payload, measured as stored `Option<u8>` bytes.
pub const MAX_HISTORY_BYTES: usize = 8 * 1024 * 1024;
/// Maximum combined undo/redo commands, also bounding record overhead.
pub const MAX_HISTORY_ENTRIES: usize = 4096;

/// Bounded random access supplied by a platform adapter.
///
/// A source reports its fixed logical length. Implementations may return short
/// reads; zero before that length is reached is treated as truncation. Native
/// adapters are responsible for file identity and external-change detection.
/// Source implementations must not return a count larger than `out.len()`.
pub trait ByteSource {
    fn len(&self) -> u64;

    fn read_at(&mut self, offset: FileOffset, out: &mut [u8]) -> io::Result<usize>;

    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// An owned source useful for tests, imported browser bytes, and small fixtures.
#[derive(Clone, Debug, Default)]
pub struct MemorySource {
    bytes: Vec<u8>,
}

impl MemorySource {
    #[must_use]
    pub fn new(bytes: Vec<u8>) -> Self {
        Self { bytes }
    }

    #[must_use]
    pub fn as_slice(&self) -> &[u8] {
        &self.bytes
    }

    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

impl From<Vec<u8>> for MemorySource {
    fn from(bytes: Vec<u8>) -> Self {
        Self::new(bytes)
    }
}

impl ByteSource for MemorySource {
    fn len(&self) -> u64 {
        self.bytes.len() as u64
    }

    fn read_at(&mut self, offset: FileOffset, out: &mut [u8]) -> io::Result<usize> {
        let start = usize::try_from(offset.get()).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidInput, "offset exceeds address space")
        })?;
        let bytes = self.bytes.get(start..).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "offset exceeds source length")
        })?;
        let count = out.len().min(bytes.len());
        out[..count].copy_from_slice(&bytes[..count]);
        Ok(count)
    }
}

/// One contiguous pending replacement range, in ascending file order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Patch {
    pub offset: FileOffset,
    pub bytes: Vec<u8>,
}

#[derive(Debug)]
struct Edit {
    offset: u64,
    before: Vec<Option<u8>>,
    after: Vec<Option<u8>>,
}

impl Edit {
    fn payload_bytes(&self) -> usize {
        (self.before.len() + self.after.len()) * size_of::<Option<u8>>()
    }
}

/// A fixed-length byte document with a pending overwrite transaction.
///
/// The overlay stores only bytes that differ from the source. Each history
/// record stores the prior and next overlay values for one bounded span; no
/// operation clones the whole source or the whole pending overlay. The explicit
/// limits above bound distinct overlay entries, history payload, record count,
/// and temporary read/edit buffers independently of source length.
///
/// All range, limit, and source-I/O checks precede mutation. Undo and redo need no
/// source I/O. Discard and successful persistence reset transaction history.
#[derive(Debug)]
pub struct Document<S: ByteSource> {
    source: S,
    len: u64,
    overlay: BTreeMap<u64, u8>,
    undo: Vec<Edit>,
    redo: Vec<Edit>,
    history_bytes: usize,
    generation: u64,
}

impl<S: ByteSource> Document<S> {
    #[must_use]
    pub fn new(source: S) -> Self {
        let len = source.len();
        Self {
            source,
            len,
            overlay: BTreeMap::new(),
            undo: Vec::new(),
            redo: Vec::new(),
            history_bytes: 0,
            generation: 0,
        }
    }

    #[must_use]
    pub const fn len(&self) -> u64 {
        self.len
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[must_use]
    pub fn is_dirty(&self) -> bool {
        !self.overlay.is_empty()
    }

    /// Report whether this file byte differs from the source in the pending overlay.
    #[must_use]
    pub fn is_byte_modified(&self, offset: FileOffset) -> bool {
        self.overlay.contains_key(&offset.get())
    }

    /// Logical byte revision. Searches reject the exhausted `u64::MAX` state.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    #[must_use]
    pub const fn source(&self) -> &S {
        &self.source
    }

    /// Read logical bytes, including pending edits, up to the end of the file.
    ///
    /// A request may contain at most [`MAX_READ_BYTES`] bytes. Short source reads
    /// are completed; premature EOF is an error, never an invented zero byte.
    /// On failure the caller's output buffer may have been partly filled, while
    /// the document and its history remain unchanged.
    pub fn read_at(&mut self, offset: FileOffset, out: &mut [u8]) -> Result<usize, Error> {
        if out.len() > MAX_READ_BYTES {
            return Err(Error::LimitExceeded {
                resource: "read buffer",
                limit: MAX_READ_BYTES,
            });
        }
        self.check_range(offset, 0)?;
        let count = out
            .len()
            .min((self.len - offset.get()).min(MAX_READ_BYTES as u64) as usize);
        self.read_source_exact(offset, &mut out[..count])?;
        let end = offset.get() + count as u64;
        for (&position, &byte) in self.overlay.range(offset.get()..end) {
            out[(position - offset.get()) as usize] = byte;
        }
        Ok(count)
    }

    /// Atomically replace an existing bounded byte range without resizing.
    ///
    /// A no-op preserves redo history and generation. A real overwrite clears
    /// redo history only after all checks and source reads have succeeded.
    pub fn overwrite(&mut self, offset: FileOffset, bytes: &[u8]) -> Result<(), Error> {
        if bytes.len() > MAX_EDIT_BYTES {
            return Err(Error::LimitExceeded {
                resource: "overwrite span",
                limit: MAX_EDIT_BYTES,
            });
        }
        self.check_range(offset, bytes.len() as u64)?;
        if bytes.is_empty() {
            return Ok(());
        }

        let mut original = vec![0; bytes.len()];
        self.read_source_exact(offset, &mut original)?;
        let mut before = vec![None; bytes.len()];
        let mut after = vec![None; bytes.len()];
        let mut pending_len = self.overlay.len();
        let mut changed = false;
        for (index, (&byte, &base)) in bytes.iter().zip(&original).enumerate() {
            let position = offset.get() + index as u64;
            before[index] = self.overlay.get(&position).copied();
            after[index] = (byte != base).then_some(byte);
            changed |= before[index] != after[index];
            match (before[index], after[index]) {
                (None, Some(_)) => pending_len += 1,
                (Some(_), None) => pending_len -= 1,
                _ => {}
            }
        }
        if !changed {
            return Ok(());
        }
        if pending_len > MAX_PENDING_BYTES {
            return Err(Error::LimitExceeded {
                resource: "pending edited bytes",
                limit: MAX_PENDING_BYTES,
            });
        }
        self.commit_edit(Edit {
            offset: offset.get(),
            before,
            after,
        })
    }

    /// Restore one byte to the backing source value for this transaction.
    ///
    /// This removes that byte's pending replacement rather than undoing the
    /// last command. The restoration itself can be undone/redone. An unchanged
    /// byte preserves history and generation; source failures leave the entire
    /// transaction unchanged.
    pub fn restore_byte(&mut self, offset: FileOffset) -> Result<(), Error> {
        self.check_range(offset, 1)?;
        let Some(before) = self.overlay.get(&offset.get()).copied() else {
            return Ok(());
        };
        // Ensure the original byte remains readable before exposing it again.
        self.read_source_exact(offset, &mut [0])?;
        self.commit_edit(Edit {
            offset: offset.get(),
            before: vec![Some(before)],
            after: vec![None],
        })
    }

    /// Undo one pending command. Returns false when no command can be undone.
    pub fn undo(&mut self) -> Result<bool, Error> {
        if self.undo.is_empty() {
            return Ok(false);
        }
        let generation = self.next_generation()?;
        if let Some(edit) = self.undo.pop() {
            Self::apply(&mut self.overlay, edit.offset, &edit.before);
            self.redo.push(edit);
        }
        self.generation = generation;
        Ok(true)
    }

    /// Reapply one undone command. Returns false when redo history is empty.
    pub fn redo(&mut self) -> Result<bool, Error> {
        if self.redo.is_empty() {
            return Ok(false);
        }
        let generation = self.next_generation()?;
        if let Some(edit) = self.redo.pop() {
            Self::apply(&mut self.overlay, edit.offset, &edit.after);
            self.undo.push(edit);
        }
        self.generation = generation;
        Ok(true)
    }

    /// Discard all pending edits and history without writing the source.
    pub fn discard(&mut self) {
        if !self.overlay.is_empty() {
            self.invalidate_generation();
        }
        self.clear_transaction();
    }

    /// Adopt backing bytes after a platform adapter has completed persistence.
    ///
    /// Callers must supply the successfully persisted logical document. This
    /// method does not itself perform or report filesystem writes.
    pub fn rebase(&mut self, source: S) {
        self.len = source.len();
        self.source = source;
        self.clear_transaction();
        self.invalidate_generation();
    }

    /// Copy pending differences into sorted, non-overlapping contiguous ranges.
    ///
    /// The total byte payload is at most [`MAX_PENDING_BYTES`]. This review
    /// snapshot allocates only in proportion to pending edits, never file size.
    #[must_use]
    pub fn patches(&self) -> Vec<Patch> {
        let mut patches: Vec<Patch> = Vec::new();
        for (&offset, &byte) in &self.overlay {
            if let Some(previous) = patches.last_mut()
                && previous
                    .offset
                    .get()
                    .checked_add(previous.bytes.len() as u64)
                    == Some(offset)
            {
                previous.bytes.push(byte);
                continue;
            }
            patches.push(Patch {
                offset: FileOffset::new(offset),
                bytes: vec![byte],
            });
        }
        patches
    }

    fn commit_edit(&mut self, edit: Edit) -> Result<(), Error> {
        let retained_history_bytes =
            self.history_bytes - self.redo.iter().map(Edit::payload_bytes).sum::<usize>();
        let history_bytes = retained_history_bytes + edit.payload_bytes();
        if history_bytes > MAX_HISTORY_BYTES {
            return Err(Error::LimitExceeded {
                resource: "undo/redo payload bytes",
                limit: MAX_HISTORY_BYTES,
            });
        }
        if self.undo.len() >= MAX_HISTORY_ENTRIES {
            return Err(Error::LimitExceeded {
                resource: "undo/redo command count",
                limit: MAX_HISTORY_ENTRIES,
            });
        }
        let generation = self.next_generation()?;

        Self::apply(&mut self.overlay, edit.offset, &edit.after);
        self.redo.clear();
        self.undo.push(edit);
        self.history_bytes = history_bytes;
        self.generation = generation;
        Ok(())
    }

    fn check_range(&self, offset: FileOffset, length: u64) -> Result<(), Error> {
        match offset.get().checked_add(length) {
            Some(end) if end <= self.len => Ok(()),
            _ => Err(Error::OutOfBounds {
                offset: offset.get(),
                length,
                document_len: self.len,
            }),
        }
    }

    fn read_source_exact(&mut self, offset: FileOffset, out: &mut [u8]) -> Result<(), Error> {
        let mut completed = 0;
        while completed < out.len() {
            let next = FileOffset::new(offset.get() + completed as u64);
            let read = self.source.read_at(next, &mut out[completed..])?;
            if read == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "source ended before its declared length",
                )
                .into());
            }
            if read > out.len() - completed {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "source returned an invalid read length",
                )
                .into());
            }
            completed += read;
        }
        Ok(())
    }

    fn apply(overlay: &mut BTreeMap<u64, u8>, offset: u64, bytes: &[Option<u8>]) {
        for (index, byte) in bytes.iter().enumerate() {
            let position = offset + index as u64;
            match byte {
                Some(byte) => {
                    overlay.insert(position, *byte);
                }
                None => {
                    overlay.remove(&position);
                }
            }
        }
    }

    fn next_generation(&self) -> Result<u64, Error> {
        self.generation
            .checked_add(1)
            .ok_or(Error::GenerationExhausted)
    }

    fn invalidate_generation(&mut self) {
        // Discard and rebasing must remain available even in the exhausted
        // state. Searches always reject MAX, so saturation cannot revive one.
        self.generation = self.generation.saturating_add(1);
    }

    fn clear_transaction(&mut self) {
        self.overlay.clear();
        self.undo.clear();
        self.redo.clear();
        self.history_bytes = 0;
    }
}
