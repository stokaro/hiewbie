//! Bounded native reads and explicit, same-directory replacement saves.
//!
//! No operation writes through the source handle. Pending edits remain in the
//! portable document until the caller adopts a successfully returned source.

use std::{
    fmt,
    fs::{self, File, Metadata},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    time::SystemTime,
};

use hiewbie_core::{ByteSource, Document, FileOffset, MAX_READ_BYTES};
use same_file::Handle;

#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};

/// Open a regular source without waiting for a writer if its path is replaced
/// by a FIFO between the metadata preflight and the open. Unix ignores
/// `O_NONBLOCK` for ordinary files; the opened handle remains authoritative.
fn open_regular(path: &Path) -> io::Result<File> {
    // Avoid opening known devices as well as FIFOs. The handle check below is
    // still required because the path can change after this preflight.
    Fingerprint::read(&fs::metadata(path)?)?;
    let mut options = File::options();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_NONBLOCK);
    let file = options.open(path)?;
    Fingerprint::read(&file.metadata()?)?;
    Ok(file)
}

/// Metadata that must stay unchanged while a document is open.
/// Access time is deliberately excluded because reading can update it.
#[derive(Debug, Eq, PartialEq)]
struct Fingerprint {
    len: u64,
    modified: SystemTime,
    created: Option<SystemTime>,
    read_only: bool,
    #[cfg(unix)]
    unix: (u64, u64, u64, u32, u32, u32, i64, i64),
}

impl Fingerprint {
    fn read(metadata: &Metadata) -> io::Result<Self> {
        if !metadata.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "only regular files are supported in this version",
            ));
        }
        Ok(Self {
            len: metadata.len(),
            modified: metadata.modified()?,
            created: metadata.created().ok(),
            read_only: metadata.permissions().readonly(),
            #[cfg(unix)]
            unix: (
                metadata.dev(),
                metadata.ino(),
                metadata.nlink(),
                metadata.mode(),
                metadata.uid(),
                metadata.gid(),
                metadata.ctime(),
                metadata.ctime_nsec(),
            ),
        })
    }
}

/// A read-only byte adapter with a fixed length and external-change checks.
#[derive(Debug)]
pub struct FileSource {
    path: PathBuf,
    handle: Handle,
    fingerprint: Fingerprint,
}

impl FileSource {
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = std::path::absolute(path)?;
        let file = open_regular(&path)?;
        Self::from_file(path, file)
    }

    fn from_file(path: PathBuf, file: File) -> io::Result<Self> {
        let handle = Handle::from_file(file)?;
        let fingerprint = Fingerprint::read(&handle.as_file().metadata()?)?;
        let source = Self {
            path,
            handle,
            fingerprint,
        };
        source.check_unchanged()?;
        Ok(source)
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Check both the open handle and the file currently named by the path.
    fn check_unchanged(&self) -> io::Result<()> {
        let current = Handle::from_file(open_regular(&self.path)?)?;
        let same_metadata = |file: &File| -> io::Result<bool> {
            Ok(Fingerprint::read(&file.metadata()?)? == self.fingerprint)
        };
        if current != self.handle
            || !same_metadata(self.handle.as_file())?
            || !same_metadata(current.as_file())?
        {
            return Err(io::Error::other(
                "the source file changed or was replaced; reopen it before editing or saving",
            ));
        }
        Ok(())
    }

    fn check_save_policy(&self) -> Result<Metadata, SaveError> {
        save_io(
            "checking source identity and metadata",
            self.check_unchanged(),
        )?;
        if save_io(
            "checking the source directory entry",
            fs::symlink_metadata(&self.path),
        )?
        .file_type()
        .is_symlink()
        {
            return Err(SaveError::Unsupported(
                "saving through a symbolic link is not supported; open its target directly",
            ));
        }
        let metadata = save_io(
            "reading source permissions",
            self.handle.as_file().metadata(),
        )?;
        if metadata.permissions().readonly() {
            return Err(SaveError::Unsupported("the source file is read-only"));
        }
        #[cfg(unix)]
        let links = metadata.nlink();
        #[cfg(windows)]
        let links = save_io(
            "checking the source hard-link count",
            winapi_util::file::information(self.handle.as_file()),
        )?
        .number_of_links();
        #[cfg(any(unix, windows))]
        if links != 1 {
            return Err(SaveError::Unsupported(
                "replacement saving of a hard-linked file is not supported",
            ));
        }
        Ok(metadata)
    }
}

impl ByteSource for FileSource {
    fn len(&self) -> u64 {
        self.fingerprint.len
    }

    fn read_at(&mut self, offset: FileOffset, out: &mut [u8]) -> io::Result<usize> {
        if offset.get() > self.len() || out.len() > MAX_READ_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "file read exceeds its offset or buffer limit",
            ));
        }
        self.check_unchanged()?;
        let count = out
            .len()
            .min((self.len() - offset.get()).min(MAX_READ_BYTES as u64) as usize);
        let file = self.handle.as_file_mut();
        file.seek(SeekFrom::Start(offset.get()))?;
        let read = loop {
            match file.read(&mut out[..count]) {
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                result => break result?,
            }
        };
        self.check_unchanged()?;
        Ok(read)
    }
}

/// A save failure. The special post-replacement variant makes the commit point
/// visible instead of incorrectly claiming that a completed write was rolled back.
#[derive(Debug)]
pub enum SaveError {
    Io(io::Error),
    Document(hiewbie_core::Error),
    Unsupported(&'static str),
    ReplacedButRefreshFailed(io::Error),
}

impl fmt::Display for SaveError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "save failed before replacement: {error}"),
            Self::Document(error) => write!(formatter, "save failed before replacement: {error}"),
            Self::Unsupported(reason) => write!(formatter, "save refused: {reason}"),
            Self::ReplacedButRefreshFailed(error) => write!(
                formatter,
                "file was replaced, but its new state could not be verified; reopen it: {error}"
            ),
        }
    }
}

impl std::error::Error for SaveError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) | Self::ReplacedButRefreshFailed(error) => Some(error),
            Self::Document(error) => Some(error),
            Self::Unsupported(_) => None,
        }
    }
}

impl From<io::Error> for SaveError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<hiewbie_core::Error> for SaveError {
    fn from(error: hiewbie_core::Error) -> Self {
        Self::Document(error)
    }
}

#[derive(Debug)]
struct SaveIoContext {
    operation: &'static str,
    error: io::Error,
}

impl fmt::Display for SaveIoContext {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.operation, self.error)
    }
}

impl std::error::Error for SaveIoContext {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}

fn save_io<T>(operation: &'static str, result: io::Result<T>) -> Result<T, SaveError> {
    result.map_err(|error| {
        SaveError::Io(io::Error::new(
            error.kind(),
            SaveIoContext { operation, error },
        ))
    })
}

/// Stream logical bytes to a sibling temporary file, sync it, check the source
/// again, and atomically replace the original path. The document is not rebased
/// here: call `Editor::complete_save` only after this returns `Ok`.
///
/// This first strategy preserves ordinary permissions and Unix ownership, but
/// does not promise ACL/xattr preservation, directory-fsync crash durability, or
/// a cross-process compare-and-swap. A writer racing the final check/rename is
/// outside the guarantee. Saving is synchronous and uses a 64KiB copy buffer.
pub fn save_document(document: &mut Document<FileSource>) -> Result<FileSource, SaveError> {
    let metadata = document.source().check_save_policy()?;
    let path = document.source().path().to_path_buf();
    if !document.is_dirty() {
        // A clean edit session must not rewrite the file or change its mtime.
        return save_io("refreshing the clean edit session", FileSource::open(path));
    }
    let parent = path.parent().ok_or(SaveError::Unsupported(
        "the source path has no parent directory",
    ))?;
    let mut temporary = save_io(
        "creating the temporary save file",
        tempfile::Builder::new()
            .prefix(".hiewbie-")
            .tempfile_in(parent),
    )?;
    #[cfg(unix)]
    {
        let temp_metadata = save_io(
            "checking temporary file ownership",
            temporary.as_file().metadata(),
        )?;
        if temp_metadata.uid() != metadata.uid() || temp_metadata.gid() != metadata.gid() {
            return Err(SaveError::Unsupported(
                "replacement would change file ownership; this save strategy cannot preserve it",
            ));
        }
    }
    copy_document(document, temporary.as_file_mut())?;
    save_io(
        "preserving file permissions",
        temporary.as_file().set_permissions(metadata.permissions()),
    )?;
    #[cfg(windows)]
    let temporary = normalize_temporary_attributes(temporary)?;
    save_io(
        "syncing the temporary save file",
        temporary.as_file().sync_all(),
    )?;
    document.source().check_save_policy()?;
    save_io(
        "checking temporary file identity",
        check_temporary_identity(&temporary),
    )?;
    let persisted = commit_temporary(temporary, &path)?;
    FileSource::from_file(path, persisted).map_err(SaveError::ReplacedButRefreshFailed)
}

#[cfg(windows)]
fn normalize_temporary_attributes(
    temporary: tempfile::NamedTempFile,
) -> Result<tempfile::NamedTempFile, SaveError> {
    save_io(
        "checking temporary file identity before clearing its attribute",
        check_temporary_identity(&temporary),
    )?;
    // Keep removes FILE_ATTRIBUTE_TEMPORARY on Windows. Restore automatic
    // cleanup immediately; it must remain enabled until replacement succeeds.
    let (file, path) = save_io(
        "clearing the temporary file attribute",
        temporary.keep().map_err(|error| error.error),
    )?;
    Ok(tempfile::NamedTempFile::from_parts(
        file,
        tempfile::TempPath::from_path(path),
    ))
}

fn commit_temporary(temporary: tempfile::NamedTempFile, path: &Path) -> Result<File, SaveError> {
    #[cfg(windows)]
    {
        // tempfile 3.23 uses MoveFileExW alone, which cannot replace our open
        // source. Rust 1.90 fs::rename retries access-denied with POSIX rename
        // semantics while retaining existing handles to the original bytes.
        let (file, mut cleanup) = temporary.into_parts();
        save_io("replacing the original file", fs::rename(&cleanup, path))?;
        cleanup.disable_cleanup(true);
        Ok(file)
    }
    #[cfg(not(windows))]
    save_io(
        "replacing the original file",
        temporary.persist(path).map_err(|error| error.error),
    )
}

fn check_temporary_identity(temporary: &tempfile::NamedTempFile) -> io::Result<()> {
    // Persist renames this directory entry. A symlink to the correct inode is
    // insufficient: it would replace the original with the symlink itself.
    Fingerprint::read(&fs::symlink_metadata(temporary.path())?)?;
    let written = Handle::from_file(temporary.as_file().try_clone()?)?;
    let named = Handle::from_file(open_regular(temporary.path())?)?;
    if written != named {
        return Err(io::Error::other(
            "the temporary save file was replaced; the original was not changed",
        ));
    }
    Ok(())
}

fn copy_document<S: ByteSource>(
    document: &mut Document<S>,
    output: &mut impl Write,
) -> Result<(), SaveError> {
    let mut buffer = vec![0; MAX_READ_BYTES];
    let mut offset = 0;
    while offset < document.len() {
        let read = document.read_at(FileOffset::new(offset), &mut buffer)?;
        if read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "source truncated during save",
            )
            .into());
        }
        save_io(
            "writing the temporary save file",
            output.write_all(&buffer[..read]),
        )?;
        offset += read as u64;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use hiewbie_core::MemorySource;

    #[test]
    fn save_io_errors_retain_the_operation_kind_and_original_error() {
        let error = save_io::<()>(
            "replacing the original file",
            Err(io::Error::from(io::ErrorKind::PermissionDenied)),
        )
        .unwrap_err();
        assert!(error.to_string().contains("replacing the original file"));
        let SaveError::Io(error) = error else {
            panic!("expected an I/O error")
        };
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        let context = error.get_ref().unwrap();
        assert!(context.source().unwrap().is::<io::Error>());
    }

    #[cfg(windows)]
    #[test]
    fn failed_windows_replacement_cleans_up_the_normalized_temporary() {
        let directory = tempfile::tempdir().unwrap();
        let temporary = tempfile::NamedTempFile::new_in(directory.path()).unwrap();
        let original_path = temporary.path().to_path_buf();
        let destination = directory.path().join("nonempty-directory");
        fs::create_dir(&destination).unwrap();
        fs::write(destination.join("original"), b"untouched").unwrap();
        let temporary = normalize_temporary_attributes(temporary).unwrap();
        assert!(commit_temporary(temporary, &destination).is_err());
        assert!(!original_path.exists());
        assert_eq!(
            fs::read(destination.join("original")).unwrap(),
            b"untouched"
        );
    }

    #[test]
    fn replaced_temporary_path_is_rejected_before_commit() {
        let directory = tempfile::tempdir().unwrap();
        let temporary = tempfile::NamedTempFile::new_in(directory.path()).unwrap();
        check_temporary_identity(&temporary).unwrap();
        fs::remove_file(temporary.path()).unwrap();
        fs::write(temporary.path(), b"unexpected").unwrap();
        assert!(check_temporary_identity(&temporary).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn temporary_symlink_to_the_written_inode_is_rejected() {
        use std::os::unix::fs::symlink;

        let directory = tempfile::tempdir().unwrap();
        let temporary = tempfile::NamedTempFile::new_in(directory.path()).unwrap();
        let alias = directory.path().join("written-inode");
        fs::hard_link(temporary.path(), &alias).unwrap();
        fs::remove_file(temporary.path()).unwrap();
        symlink(&alias, temporary.path()).unwrap();
        assert!(check_temporary_identity(&temporary).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn fifo_initial_source_and_temporary_paths_are_refused() {
        use std::{
            os::unix::fs::FileTypeExt,
            process::{Command, Stdio},
            thread,
            time::{Duration, Instant},
        };

        const CHILD_ENV: &str = "HIEWBIE_FIFO_REGRESSION_CHILD";
        if std::env::var_os(CHILD_ENV).is_none() {
            // A future blocking-open regression fails within a deadline rather
            // than hanging the test runner on a FIFO with no connected writer.
            let mut child = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "file::tests::fifo_initial_source_and_temporary_paths_are_refused",
                    "--nocapture",
                ])
                .env(CHILD_ENV, "1")
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                if child.try_wait().unwrap().is_some() {
                    let output = child.wait_with_output().unwrap();
                    assert!(
                        output.status.success(),
                        "FIFO child failed: {}{}",
                        String::from_utf8_lossy(&output.stdout),
                        String::from_utf8_lossy(&output.stderr)
                    );
                    return;
                }
                if Instant::now() >= deadline {
                    child.kill().unwrap();
                    child.wait().unwrap();
                    panic!("regular-file validation blocked while opening a FIFO");
                }
                thread::sleep(Duration::from_millis(10));
            }
        }

        fn fifo(path: &Path) {
            let result = Command::new("mkfifo").arg(path).status().unwrap();
            assert!(result.success());
        }

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("source");
        fifo(&path);
        assert!(FileSource::open(&path).is_err());
        fs::remove_file(&path).unwrap();

        fs::write(&path, b"original").unwrap();
        let mut document = Document::new(FileSource::open(&path).unwrap());
        document.overwrite(FileOffset::new(0), b"O").unwrap();
        fs::remove_file(&path).unwrap();
        fifo(&path);
        assert!(document.read_at(FileOffset::new(0), &mut [0; 1]).is_err());
        assert!(save_document(&mut document).is_err());
        assert!(fs::symlink_metadata(&path).unwrap().file_type().is_fifo());
        assert!(document.is_dirty());
        assert!(document.undo().unwrap());

        let temporary = tempfile::NamedTempFile::new_in(directory.path()).unwrap();
        fs::remove_file(temporary.path()).unwrap();
        fifo(temporary.path());
        assert!(check_temporary_identity(&temporary).is_err());
    }

    #[test]
    fn a_failing_writer_does_not_consume_the_transaction() {
        struct Full;
        impl Write for Full {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::Error::other("simulated full disk"))
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let mut document = Document::new(MemorySource::new(vec![1; MAX_READ_BYTES + 5]));
        document
            .overwrite(FileOffset::new(MAX_READ_BYTES as u64), &[2])
            .unwrap();
        assert!(copy_document(&mut document, &mut Full).is_err());
        assert!(document.is_dirty());
        assert!(document.undo().unwrap());
        assert!(!document.is_dirty());
    }
}
