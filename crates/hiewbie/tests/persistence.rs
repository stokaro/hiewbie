use std::{
    fs,
    time::{Duration, SystemTime},
};

use hiewbie::file::{FileSource, save_document};
use hiewbie_core::{ByteSource, Document, FileOffset, MAX_READ_BYTES};

fn fixture(bytes: &[u8]) -> (tempfile::TempDir, std::path::PathBuf, Document<FileSource>) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("данные-試験.bin");
    fs::write(&path, bytes).unwrap();
    let document = Document::new(FileSource::open(&path).unwrap());
    (directory, path, document)
}

#[test]
fn save_preserves_untouched_bytes_across_copy_boundaries() {
    let original: Vec<_> = (0..MAX_READ_BYTES * 2 + 9).map(|i| i as u8).collect();
    let (_directory, path, mut document) = fixture(&original);
    let offset = MAX_READ_BYTES as u64 - 1;
    document
        .overwrite(FileOffset::new(offset), &[0xAB, 0xCD, 0xEF])
        .unwrap();
    assert_eq!(fs::read(&path).unwrap(), original);
    let replacement = save_document(&mut document).unwrap();
    // Persistence does not silently clear the caller's transaction.
    assert!(document.is_dirty());
    document.rebase(replacement);
    assert!(!document.is_dirty());
    assert!(!document.undo().unwrap());
    let mut expected = original;
    expected[offset as usize..offset as usize + 3].copy_from_slice(&[0xAB, 0xCD, 0xEF]);
    assert_eq!(fs::read(&path).unwrap(), expected);
    let mut bytes = [0; 3];
    document
        .read_at(FileOffset::new(offset), &mut bytes)
        .unwrap();
    assert_eq!(bytes, [0xAB, 0xCD, 0xEF]);
}

#[test]
fn save_replaces_the_path_while_existing_readers_retain_original_bytes() {
    use std::io::Read;

    let (_directory, path, mut document) = fixture(b"original bytes");
    let mut original_reader = fs::File::open(&path).unwrap();
    document.overwrite(FileOffset::new(0), b"EDIT").unwrap();
    let replacement = save_document(&mut document).unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"EDITinal bytes");
    let mut original = Vec::new();
    original_reader.read_to_end(&mut original).unwrap();
    assert_eq!(original, b"original bytes");
    assert!(document.is_dirty());
    document.rebase(replacement);
    assert!(!document.is_dirty());
    let mut saved = [0; 4];
    document.read_at(FileOffset::new(0), &mut saved).unwrap();
    assert_eq!(&saved, b"EDIT");
}

#[test]
fn discard_and_clean_save_do_not_rewrite_the_original() {
    let (_directory, path, mut document) = fixture(b"original");
    let modified = fs::metadata(&path).unwrap().modified().unwrap();
    document.overwrite(FileOffset::new(0), b"O").unwrap();
    document.discard();
    let replacement = save_document(&mut document).unwrap();
    document.rebase(replacement);
    assert_eq!(fs::read(&path).unwrap(), b"original");
    assert_eq!(fs::metadata(path).unwrap().modified().unwrap(), modified);
}

#[test]
fn external_truncation_is_not_overwritten_and_undo_survives() {
    let (_directory, path, mut document) = fixture(b"original");
    document.overwrite(FileOffset::new(0), b"O").unwrap();
    fs::write(&path, b"new").unwrap();
    assert!(save_document(&mut document).is_err());
    assert_eq!(fs::read(path).unwrap(), b"new");
    assert!(document.is_dirty());
    assert!(document.undo().unwrap());
    assert!(!document.is_dirty());
}

#[test]
fn same_length_external_write_is_detected() {
    let (_directory, path, mut document) = fixture(b"original");
    document.overwrite(FileOffset::new(0), b"O").unwrap();
    fs::write(&path, b"external").unwrap();
    fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(1_234_567))
        .unwrap();
    assert!(save_document(&mut document).is_err());
    assert!(document.read_at(FileOffset::new(0), &mut [0; 1]).is_err());
    assert_eq!(fs::read(path).unwrap(), b"external");
    assert!(document.is_dirty());
}

#[test]
fn external_replacement_with_the_same_length_is_detected() {
    let (directory, path, mut document) = fixture(b"original");
    document.overwrite(FileOffset::new(0), b"O").unwrap();
    let replacement = directory.path().join("replacement");
    fs::write(&replacement, b"external").unwrap();
    fs::rename(replacement, &path).unwrap();
    assert!(save_document(&mut document).is_err());
    assert_eq!(fs::read(path).unwrap(), b"external");
    assert!(document.is_dirty());
}

#[test]
fn missing_source_is_not_recreated_from_stale_data() {
    let (_directory, path, mut document) = fixture(b"original");
    document.overwrite(FileOffset::new(0), b"O").unwrap();
    fs::remove_file(&path).unwrap();
    assert!(save_document(&mut document).is_err());
    assert!(!path.exists());
    assert!(document.is_dirty());
}

#[test]
fn read_only_files_cannot_be_replaced() {
    let (_directory, path, document) = fixture(b"original");
    drop(document);
    let permissions = fs::metadata(&path).unwrap().permissions();
    let mut read_only = permissions.clone();
    read_only.set_readonly(true);
    fs::set_permissions(&path, read_only).unwrap();
    let mut document = Document::new(FileSource::open(&path).unwrap());
    document.overwrite(FileOffset::new(0), b"O").unwrap();
    let result = save_document(&mut document);
    // Restore before assertions so Windows can clean up the temporary folder.
    fs::set_permissions(&path, permissions).unwrap();
    assert!(result.is_err());
    assert!(document.is_dirty());
    assert_eq!(fs::read(path).unwrap(), b"original");
}

#[test]
fn empty_files_open_without_inventing_bytes() {
    let (_directory, path, mut document) = fixture(b"");
    assert!(document.is_empty());
    assert_eq!(
        document.read_at(FileOffset::new(0), &mut [0; 1]).unwrap(),
        0
    );
    assert!(document.overwrite(FileOffset::new(0), b"x").is_err());
    assert_eq!(save_document(&mut document).unwrap().len(), 0);
    assert_eq!(fs::metadata(path).unwrap().len(), 0);
}

#[test]
fn nonregular_paths_are_rejected() {
    let directory = tempfile::tempdir().unwrap();
    assert!(FileSource::open(directory.path()).is_err());
}

#[cfg(windows)]
mod windows {
    use super::*;
    use hiewbie::file::SaveError;

    #[test]
    fn existing_hardlinks_are_refused_without_detaching_an_alias() {
        let (directory, path, document) = fixture(b"original");
        drop(document);
        let alias = directory.path().join("hard-link");
        fs::hard_link(&path, &alias).unwrap();
        let mut document = Document::new(FileSource::open(&alias).unwrap());
        document.overwrite(FileOffset::new(0), b"O").unwrap();
        assert!(matches!(
            save_document(&mut document),
            Err(SaveError::Unsupported(_))
        ));
        assert_eq!(fs::read(&path).unwrap(), b"original");
        assert_eq!(fs::read(&alias).unwrap(), b"original");
        assert!(same_file::is_same_file(&path, &alias).unwrap());
        assert!(document.is_dirty());
    }

    #[test]
    fn a_hardlink_added_during_editing_prevents_replacement() {
        let (directory, path, mut document) = fixture(b"original");
        document.overwrite(FileOffset::new(0), b"O").unwrap();
        let alias = directory.path().join("late-hard-link");
        fs::hard_link(&path, &alias).unwrap();
        assert!(save_document(&mut document).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"original");
        assert_eq!(fs::read(&alias).unwrap(), b"original");
        assert!(same_file::is_same_file(&path, &alias).unwrap());
        assert!(document.is_dirty());
        assert!(document.undo().unwrap());
    }
}

#[cfg(unix)]
mod unix {
    use super::*;
    use std::io::{Read, Seek, SeekFrom, Write};
    use std::os::unix::fs::{PermissionsExt, symlink};

    #[test]
    fn executable_mode_is_preserved() {
        let (_directory, path, document) = fixture(b"original");
        drop(document);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o751)).unwrap();
        let mut document = Document::new(FileSource::open(&path).unwrap());
        document.overwrite(FileOffset::new(0), b"O").unwrap();
        save_document(&mut document).unwrap();
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o7777,
            0o751
        );
    }

    #[test]
    fn symlink_and_hardlink_saves_leave_all_aliases_unchanged() {
        let (directory, path, document) = fixture(b"original");
        drop(document);
        let symbolic = directory.path().join("symbolic");
        symlink(&path, &symbolic).unwrap();
        let mut linked = Document::new(FileSource::open(&symbolic).unwrap());
        linked.overwrite(FileOffset::new(0), b"O").unwrap();
        assert!(save_document(&mut linked).is_err());
        assert!(
            fs::symlink_metadata(&symbolic)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        let hard = directory.path().join("hard");
        fs::hard_link(&path, &hard).unwrap();
        let mut linked = Document::new(FileSource::open(&hard).unwrap());
        linked.overwrite(FileOffset::new(0), b"O").unwrap();
        assert!(save_document(&mut linked).is_err());
        for name in [path, symbolic, hard] {
            assert_eq!(fs::read(name).unwrap(), b"original");
        }
    }

    #[test]
    fn sparse_file_offsets_beyond_four_gib_are_read_without_a_full_file_buffer() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("large-sparse.bin");
        let offset = (1_u64 << 32) + 17;
        {
            let mut file = fs::File::create(&path).unwrap();
            file.set_len(offset + 3).unwrap();
            file.seek(SeekFrom::Start(offset)).unwrap();
            file.write_all(b"END").unwrap();
        }
        let mut document = Document::new(FileSource::open(&path).unwrap());
        let mut bytes = [0; 3];
        assert_eq!(
            document
                .read_at(FileOffset::new(offset), &mut bytes)
                .unwrap(),
            3
        );
        assert_eq!(&bytes, b"END");
        document
            .overwrite(FileOffset::new(offset + 1), b"X")
            .unwrap();
        document
            .read_at(FileOffset::new(offset), &mut bytes)
            .unwrap();
        assert_eq!(&bytes, b"EXD");
        document.discard();
        let mut file = fs::File::open(path).unwrap();
        file.seek(SeekFrom::Start(offset)).unwrap();
        file.read_exact(&mut bytes).unwrap();
        assert_eq!(&bytes, b"END");
    }
}
