use std::{cell::Cell, io, rc::Rc};

use crate::symbols::{SymbolKind, SymbolTarget};
use crate::*;

use crate::symbol_fixtures as fixtures;

fn editor(bytes: Vec<u8>) -> Editor<MemorySource> {
    Editor::new(Document::new(MemorySource::new(bytes)), false)
}

fn named(editor: &mut Editor<MemorySource>, name: &str) -> usize {
    editor.refresh_symbols();
    editor
        .symbols()
        .unwrap()
        .entries
        .iter()
        .position(|symbol| symbol.name == name)
        .unwrap()
}

#[test]
fn symbol_target_record_and_unresolved_commands_preserve_byte_transactions() {
    let bytes = fixtures::pe64();
    let mut editor = editor(bytes.clone());
    let import = named(&mut editor, "external");
    assert_eq!(
        editor.symbols().unwrap().entries[import].kind,
        SymbolKind::Import
    );
    assert!(matches!(
        editor.symbols().unwrap().entries[import].target,
        SymbolTarget::ImportSlot(_)
    ));
    editor.dispatch(Command::GotoSymbol(import)).unwrap();
    assert_eq!(editor.cursor().get(), 0x360);
    editor.dispatch(Command::GotoSymbolRecord(import)).unwrap();
    assert_eq!(editor.cursor().get(), 0x340);
    let export = named(&mut editor, "entry_symbol");
    editor.dispatch(Command::GotoSymbol(export)).unwrap();
    assert_eq!(editor.cursor().get(), 0x200);
    let context = editor.decode_context();
    let forwarded = editor
        .symbols()
        .unwrap()
        .entries
        .iter()
        .position(|symbol| symbol.target == SymbolTarget::Unavailable)
        .unwrap();
    assert!(editor.dispatch(Command::GotoSymbol(forwarded)).is_err());
    assert_eq!(editor.cursor().get(), 0x200);
    assert_eq!(editor.decode_context(), context);
    editor
        .dispatch(Command::GotoSymbolRecord(forwarded))
        .unwrap();
    assert_eq!(editor.cursor().get(), 0x444);
    assert!(editor.dispatch(Command::GotoSymbol(usize::MAX)).is_err());
    let mut after = vec![0; bytes.len()];
    editor
        .document_mut()
        .read_at(FileOffset::new(0), &mut after)
        .unwrap();
    assert_eq!(after, bytes);
    assert!(!editor.document().is_dirty());
}

#[test]
fn names_and_targets_refresh_after_edit_restore_undo_redo_discard_and_save() {
    let mut editor = editor(fixtures::pe64());
    named(&mut editor, "entry_symbol");
    editor.dispatch(Command::BeginEdit).unwrap();
    editor
        .dispatch(Command::Goto(FileOffset::new(0x460)))
        .unwrap();
    editor.dispatch(Command::WriteByte(b'E')).unwrap();
    assert!(editor.symbols().is_none());
    named(&mut editor, "Entry_symbol");
    editor.dispatch(Command::Undo).unwrap();
    named(&mut editor, "entry_symbol");
    editor.dispatch(Command::Redo).unwrap();
    named(&mut editor, "Entry_symbol");
    editor
        .dispatch(Command::Goto(FileOffset::new(0x460)))
        .unwrap();
    editor.dispatch(Command::RestoreByte).unwrap();
    named(&mut editor, "entry_symbol");
    editor.dispatch(Command::Undo).unwrap();
    named(&mut editor, "Entry_symbol");
    editor
        .document_mut()
        .overwrite(FileOffset::new(0x440), &0x1006_u32.to_le_bytes())
        .unwrap();
    let index = named(&mut editor, "Entry_symbol");
    editor.dispatch(Command::GotoSymbol(index)).unwrap();
    assert_eq!(editor.cursor().get(), 0x206);
    editor.dispatch(Command::DiscardEdit).unwrap();
    let index = named(&mut editor, "entry_symbol");
    editor.dispatch(Command::GotoSymbol(index)).unwrap();
    assert_eq!(editor.cursor().get(), 0x200);
    let mut saved = fixtures::pe64();
    saved[0x460] = b'S';
    editor.complete_save(MemorySource::new(saved));
    assert!(editor.symbols().is_none());
    named(&mut editor, "Sntry_symbol");
    editor.dispatch(Command::SetRaw(true)).unwrap();
    editor.refresh_symbols();
    assert!(editor.symbols().is_none());
    assert!(editor.symbol_problem().unwrap().contains("raw"));
    editor.dispatch(Command::SetRaw(false)).unwrap();
    named(&mut editor, "Sntry_symbol");
}

#[test]
fn malformed_symbol_tables_leave_headers_code_and_raw_bytes_usable() {
    let mut bytes = fixtures::pe64();
    fixtures::put32(&mut bytes, 0x300, u32::MAX);
    let mut editor = editor(bytes);
    editor.refresh_analysis();
    assert!(editor.executable().is_some());
    assert!(editor.symbols().is_none());
    editor.refresh_symbols();
    assert!(editor.symbol_problem().is_some());
    assert!(editor.analysis_problem().is_none());
    editor.dispatch(Command::GotoEntry).unwrap();
    assert_eq!(
        editor.code_rows(1).unwrap()[0].instruction.bytes(),
        &[0xeb, 4]
    );
    editor.dispatch(Command::BeginEdit).unwrap();
    editor
        .document_mut()
        .overwrite(FileOffset::new(0x300), &0x1140_u32.to_le_bytes())
        .unwrap();
    assert!(
        editor.symbol_problem().is_none(),
        "stale errors must not escape their generation"
    );
    named(&mut editor, "external");
}

#[test]
fn symbol_inspection_is_lazy_and_cached_for_one_document_generation() {
    struct Counted {
        source: MemorySource,
        symbol_reads: Rc<Cell<usize>>,
    }
    impl ByteSource for Counted {
        fn len(&self) -> u64 {
            self.source.len()
        }
        fn read_at(&mut self, offset: FileOffset, out: &mut [u8]) -> io::Result<usize> {
            if offset.get() >= 0x300 {
                self.symbol_reads.set(self.symbol_reads.get() + 1);
            }
            self.source.read_at(offset, out)
        }
    }
    let reads = Rc::new(Cell::new(0));
    let source = Counted {
        source: MemorySource::new(fixtures::pe64()),
        symbol_reads: Rc::clone(&reads),
    };
    let mut editor = Editor::new(Document::new(source), false);
    editor.refresh_analysis();
    editor.dispatch(Command::GotoEntry).unwrap();
    editor.code_rows(1).unwrap();
    assert_eq!(reads.get(), 0);
    editor.refresh_symbols();
    let parsed = reads.get();
    assert!(parsed > 0);
    for _ in 0..3 {
        editor.refresh_symbols();
    }
    assert_eq!(reads.get(), parsed);
    editor.dispatch(Command::BeginEdit).unwrap();
    editor
        .document_mut()
        .overwrite(FileOffset::new(0x460), b"E")
        .unwrap();
    editor.refresh_symbols();
    assert!(reads.get() > parsed);
    assert!(
        editor
            .symbols()
            .unwrap()
            .entries
            .iter()
            .any(|symbol| symbol.name == "Entry_symbol")
    );
}

#[test]
fn textual_assembly_resolves_symbols_from_the_current_logical_generation() {
    let mut editor = editor(fixtures::pe64());
    editor.dispatch(Command::GotoEntry).unwrap();
    assert!(editor.symbols().is_none());
    assert_eq!(
        editor.assemble_at_cursor("jmp @entry_symbol").unwrap(),
        [vec![0xE9, 0xFB, 0xFF, 0xFF, 0xFF]],
    );
    assert!(editor.symbols().is_some());
    editor.dispatch(Command::BeginEdit).unwrap();
    editor
        .document_mut()
        .overwrite(FileOffset::new(0x440), &0x1006_u32.to_le_bytes())
        .unwrap();
    assert!(editor.symbols().is_none());
    assert_eq!(
        editor.assemble_at_cursor("jmps entry_symbol").unwrap(),
        [vec![0xEB, 0x04]],
    );
    assert!(editor.document().is_dirty());
    assert_eq!(editor.cursor(), FileOffset::new(0x200));
}
