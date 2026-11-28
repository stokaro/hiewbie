use std::{cell::Cell, io, rc::Rc};

use hiewbie_core::{ByteSource, FileOffset, MAX_READ_BYTES};

/// Counts actual source bytes, including completed short reads. A sparse tail
/// lets untrusted u64 offsets/counts reach adapters without allocating the file.
pub(crate) struct Source<'a> {
    pub bytes: &'a [u8],
    pub len: u64,
    pub short: usize,
    pub read: Rc<Cell<usize>>,
    pub limit: Rc<Cell<usize>>,
}

impl ByteSource for Source<'_> {
    fn len(&self) -> u64 {
        self.len
    }

    fn read_at(&mut self, offset: FileOffset, out: &mut [u8]) -> io::Result<usize> {
        assert!(
            out.len() <= MAX_READ_BYTES,
            "source request exceeds document limit"
        );
        let available = self
            .len
            .checked_sub(offset.get())
            .expect("out-of-range source read");
        let count = out
            .len()
            .min(self.short)
            .min(available.min(MAX_READ_BYTES as u64) as usize);
        let total = self
            .read
            .get()
            .checked_add(count)
            .expect("source counter overflow");
        assert!(
            total <= self.limit.get(),
            "adapter bypassed its read budget"
        );
        self.read.set(total);
        out[..count].fill(0);
        if let Ok(start) = usize::try_from(offset.get())
            && let Some(bytes) = self.bytes.get(start..)
        {
            let copied = count.min(bytes.len());
            out[..copied].copy_from_slice(&bytes[..copied]);
        }
        Ok(count)
    }
}
