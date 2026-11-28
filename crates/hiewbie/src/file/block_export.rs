use hiewbie_core::formats::FileRange;

use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExportOffset {
    Absolute(FileOffset),
    Append,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExportProgress {
    Staging { done: u64, total: u64 },
    Complete { block_bytes: u64 },
}

/// A cancellable native export. The named output changes only at commit.
/// Dropping this value before commit removes its sibling temporary file.
#[derive(Debug)]
pub struct BlockExport {
    destination: PathBuf,
    previous: Option<FileSource>,
    temporary: Option<tempfile::NamedTempFile>,
    source_range: FileRange,
    source_generation: u64,
    destination_start: u64,
    destination_end: u64,
    output_len: u64,
    position: u64,
    buffer: Vec<u8>,
}

impl BlockExport {
    /// Prepare a private output. `Ok(None)` requests confirmation for an
    /// existing destination. A new destination is committed without clobbering
    /// a file created by another process while the export was staging.
    pub fn begin(
        document: &Document<FileSource>,
        source_range: FileRange,
        path: &Path,
        offset: ExportOffset,
        replace_existing: bool,
    ) -> Result<Option<Self>, SaveError> {
        if source_range.len == 0
            || source_range
                .end()
                .map_or(true, |end| end.get() > document.len())
        {
            return Err(SaveError::Unsupported(
                "block export requires a nonempty file range",
            ));
        }
        if document.generation() == u64::MAX {
            return Err(hiewbie_core::Error::GenerationExhausted.into());
        }
        save_io(
            "checking the block source",
            document.source().check_unchanged(),
        )?;
        let destination = save_io("resolving the output file path", std::path::absolute(path))?;
        let previous = match fs::symlink_metadata(&destination) {
            Ok(entry) if !entry.is_file() => {
                return Err(SaveError::Unsupported(
                    "block export requires a regular output path",
                ));
            }
            Ok(_) => {
                let output = save_io(
                    "opening the existing output",
                    FileSource::open(&destination),
                )?;
                if output.handle == document.source().handle {
                    return Err(SaveError::Unsupported(
                        "choose an output file distinct from the open document",
                    ));
                }
                Some(output)
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return save_io("checking the output path", Err(error)),
        };
        if previous.is_some() && !replace_existing {
            return Ok(None);
        }
        let metadata = previous
            .as_ref()
            .map(FileSource::check_save_policy)
            .transpose()?;
        let previous_len = previous.as_ref().map_or(0, FileSource::len);
        let destination_start = match offset {
            ExportOffset::Absolute(offset) => offset.get(),
            ExportOffset::Append => previous_len,
        };
        let destination_end =
            destination_start
                .checked_add(source_range.len)
                .ok_or(SaveError::Unsupported(
                    "block output exceeds the maximum file offset",
                ))?;
        let parent = destination.parent().ok_or(SaveError::Unsupported(
            "output path has no parent directory",
        ))?;
        let temporary = save_io(
            "creating the temporary block output",
            tempfile::Builder::new()
                .prefix(".hiewbie-block-")
                .tempfile_in(parent),
        )?;
        if let Some(metadata) = &metadata {
            check_replacement_owner(&temporary, metadata)?;
        }
        Ok(Some(Self {
            destination,
            previous,
            temporary: Some(temporary),
            source_range,
            source_generation: document.generation(),
            destination_start,
            destination_end,
            output_len: destination_end.max(previous_len),
            position: 0,
            buffer: vec![0; MAX_READ_BYTES],
        }))
    }

    /// Stage at most one copy buffer and commit on a subsequent event-loop tick.
    pub fn step(
        &mut self,
        document: &mut Document<FileSource>,
    ) -> Result<ExportProgress, SaveError> {
        let progress = self.advance(document);
        if progress.is_err() {
            // A failed write may have advanced the temporary handle. Discard the
            // staging file so a retry must start from the beginning.
            self.temporary = None;
        }
        progress
    }

    fn advance(
        &mut self,
        document: &mut Document<FileSource>,
    ) -> Result<ExportProgress, SaveError> {
        if self.temporary.is_none() {
            return Err(SaveError::Unsupported(
                "this block export has already finished",
            ));
        }
        if document.generation() != self.source_generation {
            return Err(SaveError::Unsupported(
                "the document changed during block export",
            ));
        }
        if self.position < self.output_len {
            self.stage_chunk(document)?;
            return Ok(ExportProgress::Staging {
                done: self.position,
                total: self.output_len,
            });
        }
        self.commit(document)?;
        Ok(ExportProgress::Complete {
            block_bytes: self.source_range.len,
        })
    }

    fn stage_chunk(&mut self, document: &mut Document<FileSource>) -> Result<(), SaveError> {
        let temporary = self.temporary.as_mut().ok_or(SaveError::Unsupported(
            "block export has no temporary output",
        ))?;
        let position = self.position;
        let previous_len = self.previous.as_ref().map_or(0, FileSource::len);
        let inside_block = (self.destination_start..self.destination_end).contains(&position);
        let boundary = if inside_block {
            self.destination_end
        } else if position < self.destination_start {
            if position >= previous_len {
                // Leave an implicit zero-filled gap. A single seek avoids work
                // proportional to an explicitly requested output offset.
                save_io(
                    "seeking to the block output offset",
                    temporary
                        .as_file_mut()
                        .seek(SeekFrom::Start(self.destination_start)),
                )?;
                self.position = self.destination_start;
                return Ok(());
            }
            self.destination_start.min(previous_len)
        } else {
            previous_len
        };
        let count = (boundary - position).min(MAX_READ_BYTES as u64) as usize;
        let output = &mut self.buffer[..count];
        if inside_block {
            let source_position = FileOffset::new(
                self.source_range.offset.get() + (position - self.destination_start),
            );
            document.read_at(source_position, output)?;
        } else if let Some(previous) = &mut self.previous {
            read_existing(previous, FileOffset::new(position), output)?;
        }
        save_io(
            "writing the temporary block output",
            temporary.as_file_mut().write_all(output),
        )?;
        self.position += count as u64;
        Ok(())
    }

    fn commit(&mut self, document: &Document<FileSource>) -> Result<(), SaveError> {
        let temporary = self.temporary.take().ok_or(SaveError::Unsupported(
            "block export has no temporary output",
        ))?;
        if let Some(previous) = &self.previous {
            let metadata = previous.check_save_policy()?;
            save_io(
                "preserving the output file permissions",
                temporary.as_file().set_permissions(metadata.permissions()),
            )?;
        }
        #[cfg(windows)]
        let temporary = normalize_temporary_attributes(temporary)?;
        save_io(
            "syncing the temporary block output",
            temporary.as_file().sync_all(),
        )?;
        save_io(
            "checking the block source before commit",
            document.source().check_unchanged(),
        )?;
        save_io(
            "checking the temporary block output",
            check_temporary_identity(&temporary),
        )?;
        if let Some(previous) = &self.previous {
            previous.check_save_policy()?;
            commit_temporary(temporary, &self.destination)?;
        } else {
            save_io(
                "creating the block output without replacing another file",
                temporary
                    .persist_noclobber(&self.destination)
                    .map_err(|error| error.error),
            )?;
        }
        Ok(())
    }
}

fn read_existing(
    source: &mut FileSource,
    start: FileOffset,
    mut buffer: &mut [u8],
) -> Result<(), SaveError> {
    let mut offset = start.get();
    while !buffer.is_empty() {
        let count = save_io(
            "reading existing output bytes",
            source.read_at(FileOffset::new(offset), buffer),
        )?;
        if count == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "the output file was truncated",
            )
            .into());
        }
        offset += count as u64;
        buffer = &mut buffer[count..];
    }
    Ok(())
}
