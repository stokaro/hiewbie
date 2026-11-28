use std::fs;
use std::path::Path;

use hiewbie::file::{BlockExport, ExportOffset, ExportProgress, FileSource};
use hiewbie_core::formats::FileRange;
use hiewbie_core::{Document, FileOffset, MAX_READ_BYTES};

fn range(first: u64, len: u64) -> FileRange {
    FileRange {
        offset: FileOffset::new(first),
        len,
    }
}

fn absolute(offset: u64) -> ExportOffset {
    ExportOffset::Absolute(FileOffset::new(offset))
}

fn export(
    document: &mut Document<FileSource>,
    block: FileRange,
    path: &Path,
    offset: ExportOffset,
    replace_existing: bool,
) {
    let mut operation = BlockExport::begin(document, block, path, offset, replace_existing)
        .unwrap()
        .unwrap();
    for _ in 0..10 {
        if operation.step(document).unwrap()
            == (ExportProgress::Complete {
                block_bytes: block.len,
            })
        {
            return;
        }
    }
    panic!("fixture export failed to complete");
}

#[test]
fn block_export_stages_bounded_logical_bytes_and_preserves_the_original() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source.bin");
    let output = directory.path().join("export.bin");
    let original: Vec<_> = (0..MAX_READ_BYTES * 2 + 9)
        .map(|index| index as u8)
        .collect();
    fs::write(&source, &original).unwrap();
    let mtime = fs::metadata(&source).unwrap().modified().unwrap();
    let mut document = Document::new(FileSource::open(&source).unwrap());
    let start = 7;
    let block = range(start, MAX_READ_BYTES as u64 + 5);
    document
        .overwrite(FileOffset::new(start + 1), &[0xFA])
        .unwrap();

    let mut operation = BlockExport::begin(&document, block, &output, absolute(2), false)
        .unwrap()
        .unwrap();
    assert_eq!(
        operation.step(&mut document).unwrap(),
        ExportProgress::Staging {
            done: 2,
            total: block.len + 2
        }
    );
    assert_eq!(
        operation.step(&mut document).unwrap(),
        ExportProgress::Staging {
            done: MAX_READ_BYTES as u64 + 2,
            total: block.len + 2,
        }
    );
    assert_eq!(
        operation.step(&mut document).unwrap(),
        ExportProgress::Staging {
            done: block.len + 2,
            total: block.len + 2,
        }
    );
    assert!(!output.exists());
    assert_eq!(
        operation.step(&mut document).unwrap(),
        ExportProgress::Complete {
            block_bytes: block.len
        }
    );
    let mut expected = vec![0, 0];
    expected.extend_from_slice(&original[start as usize..(start + block.len) as usize]);
    expected[3] = 0xFA;
    assert_eq!(fs::read(&output).unwrap(), expected);
    assert_eq!(fs::read(&source).unwrap(), original);
    assert_eq!(fs::metadata(&source).unwrap().modified().unwrap(), mtime);
    assert!(document.is_dirty());
    assert_eq!(document.patches()[0].bytes, [0xFA]);
}

#[test]
fn block_export_appends_and_overwrites_without_losing_unrelated_bytes() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source.bin");
    let output = directory.path().join("existing.bin");
    fs::write(&source, b"ABCDE").unwrap();
    fs::write(&output, b"12345").unwrap();
    let mut document = Document::new(FileSource::open(&source).unwrap());
    let block = range(1, 2);
    assert!(
        BlockExport::begin(&document, block, &output, ExportOffset::Append, false)
            .unwrap()
            .is_none()
    );
    assert_eq!(fs::read(&output).unwrap(), b"12345");
    export(&mut document, block, &output, absolute(1), true);
    assert_eq!(fs::read(&output).unwrap(), b"1BC45");
    export(&mut document, block, &output, ExportOffset::Append, true);
    assert_eq!(fs::read(&output).unwrap(), b"1BC45BC");
    assert!(BlockExport::begin(&document, block, &source, absolute(0), true).is_err());
    assert!(BlockExport::begin(&document, range(4, 2), &output, absolute(0), true).is_err());
    assert!(BlockExport::begin(&document, block, &output, absolute(u64::MAX), true).is_err());
    assert_eq!(fs::read(&source).unwrap(), b"ABCDE");
    assert_eq!(fs::read(&output).unwrap(), b"1BC45BC");
    assert!(!document.is_dirty());
}

#[test]
fn canceled_and_conflicting_block_exports_leave_outputs_and_no_temporary_files() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source.bin");
    let output = directory.path().join("output.bin");
    fs::write(&source, b"ABCDE").unwrap();
    fs::write(&output, b"12345").unwrap();
    let mut document = Document::new(FileSource::open(&source).unwrap());
    let block = range(0, 3);
    let mut canceled = BlockExport::begin(&document, block, &output, ExportOffset::Append, true)
        .unwrap()
        .unwrap();
    canceled.step(&mut document).unwrap();
    canceled.step(&mut document).unwrap();
    drop(canceled);
    assert_eq!(fs::read(&output).unwrap(), b"12345");
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 2);

    let mut conflict = BlockExport::begin(&document, block, &output, absolute(0), true)
        .unwrap()
        .unwrap();
    conflict.step(&mut document).unwrap();
    conflict.step(&mut document).unwrap();
    fs::write(&output, b"external change").unwrap();
    assert!(conflict.step(&mut document).is_err());
    assert!(conflict.step(&mut document).is_err());
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 2);
    assert_eq!(fs::read(&output).unwrap(), b"external change");

    let new_path = directory.path().join("new.bin");
    let mut concurrent = BlockExport::begin(&document, block, &new_path, absolute(0), false)
        .unwrap()
        .unwrap();
    concurrent.step(&mut document).unwrap();
    fs::write(&new_path, b"external creation").unwrap();
    assert!(concurrent.step(&mut document).is_err());
    drop(concurrent);
    assert_eq!(fs::read(&new_path).unwrap(), b"external creation");
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 3);
}
