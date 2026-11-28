use super::*;
use crate::formats::{self, Region};
use crate::{Command, Editor, MemorySource};

use crate::symbol_fixtures as fixtures;

fn document() -> (Document<MemorySource>, Executable) {
    let mut document = Document::new(MemorySource::new(fixtures::pe64()));
    let image = formats::inspect(&mut document, None).unwrap().unwrap();
    (document, image)
}

fn reader(document: &mut Document<MemorySource>, slice: FileRange) -> Reader<'_, MemorySource> {
    Reader {
        document,
        slice,
        read_bytes: 0,
        text_bytes: 0,
    }
}

#[test]
fn symbol_reader_rejects_slice_and_arithmetic_overflow_before_reading() {
    let (mut document, image) = document();
    let reader = reader(&mut document, image.slice);
    for (offset, length) in [(0x800, 1), (0x7ff, 2), (u64::MAX, 2)] {
        assert!(reader.range(offset, length).is_err());
    }
    assert_eq!(reader.range(0x800, 0).unwrap().offset.get(), 0x800);
    assert!(reader.table(0, MAX_SYMBOLS as u64 + 1, 1).is_err());
    assert!(reader.table(0, 2, u64::MAX).is_err());
    assert!(reader.table(0, 1, 0).is_err());
    assert_eq!(reader.read_bytes, 0);
}

#[test]
fn symbol_va_ranges_reject_interior_overlaps_zero_fill_and_wraparound() {
    let (mut document, mut image) = document();
    let reader = reader(&mut document, image.slice);
    let base = VirtualAddress::new(0x1_4000_1000);
    assert_eq!(reader.va(&image, base, 8).unwrap(), 0x200);
    let mut overlap: Region = image.regions.last().unwrap().clone();
    overlap.address = VirtualAddress::new(base.get() + 4);
    overlap.memory_size = 1;
    overlap.file.len = 1;
    image.regions.push(overlap);
    assert!(reader.va(&image, base, 8).is_err());
    image.regions.last_mut().unwrap().memory_size = 0;
    assert_eq!(reader.va(&image, base, 8).unwrap(), 0x200);
    image.regions.pop();
    image.regions.last_mut().unwrap().file.len = 4;
    assert!(reader.va(&image, base, 8).is_err());
    assert!(reader.va(&image, VirtualAddress::new(u64::MAX), 2).is_err());
}

#[test]
fn symbol_names_require_bounded_termination_and_limit_decoded_retained_text() {
    let mut document = Document::new(MemorySource::new(vec![b'x'; MAX_SYMBOL_NAME_BYTES + 2]));
    let slice = FileRange {
        offset: FileOffset::new(0),
        len: document.len(),
    };
    let mut reader = reader(&mut document, slice);
    assert!(matches!(
        reader.c_string(0, slice.len),
        Err(FormatError::Limit {
            resource: "symbol name bytes",
            ..
        })
    ));
    assert!(matches!(
        reader.c_string(0, 8),
        Err(FormatError::Invalid { .. })
    ));
    assert!(reader.c_string(8, 7).is_err());
    reader.read_bytes = MAX_SYMBOL_READ_BYTES;
    assert!(matches!(
        reader.read::<1>(0),
        Err(FormatError::Limit {
            resource: "symbol read bytes",
            ..
        })
    ));
    reader.text_bytes = MAX_SYMBOL_TEXT_BYTES;
    assert!(matches!(
        reader.retain_text(1),
        Err(FormatError::Limit {
            resource: "symbol text bytes",
            ..
        })
    ));
}

#[test]
fn symbol_records_and_notes_are_bounded_without_inventing_targets() {
    let (mut document, image) = document();
    let mut reader = reader(&mut document, image.slice);
    let mut table = SymbolTable::new(&image);
    let mut symbol = Symbol {
        name: "name".into(),
        kind: SymbolKind::Defined,
        library: None,
        ordinal: None,
        target: SymbolTarget::Unavailable,
        record: FileRange {
            offset: FileOffset::new(0),
            len: 0,
        },
        size: None,
        detail: String::new(),
    };
    assert!(reader.push(&mut table, symbol.clone()).is_err());
    symbol.record.len = 1;
    symbol.target = SymbolTarget::File(FileOffset::new(image.slice.len));
    assert!(reader.push(&mut table, symbol.clone()).is_err());
    symbol.target = SymbolTarget::Unavailable;
    reader.push(&mut table, symbol).unwrap();
    for index in 0..MAX_SYMBOL_NOTES {
        reader
            .note(&mut table, &format!("coverage {index}"))
            .unwrap();
    }
    reader.note(&mut table, "coverage 0").unwrap();
    assert_eq!(table.notes.len(), MAX_SYMBOL_NOTES);
    assert!(reader.note(&mut table, "one more").is_err());
}

#[test]
fn symbols_reject_stale_images_and_keep_unresolved_record_navigation() {
    let (mut document, image) = document();
    document.overwrite(FileOffset::new(0x460), b"E").unwrap();
    assert!(inspect(&mut document, &image).is_err());
    let mut editor = Editor::new(document, false);
    editor.refresh_symbols();
    let index = editor
        .symbols()
        .unwrap()
        .entries
        .iter()
        .position(|symbol| symbol.target == SymbolTarget::Unavailable)
        .unwrap();
    let record = editor.symbols().unwrap().entries[index].record;
    let cursor = editor.cursor();
    let context = editor.decode_context();
    assert!(editor.dispatch(Command::GotoSymbol(index)).is_err());
    assert_eq!(editor.cursor(), cursor);
    assert_eq!(editor.decode_context(), context);
    editor.dispatch(Command::GotoSymbolRecord(index)).unwrap();
    assert_eq!(editor.cursor(), record.offset);
}
