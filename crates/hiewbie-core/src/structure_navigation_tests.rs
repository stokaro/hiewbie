use crate::formats::{StructureKind, StructureStatus};
use crate::*;

use crate::te_fixtures as fixtures;
#[path = "../../../tests/fixtures/macho_records.rs"]
mod macho_fixtures;

fn editor(bytes: Vec<u8>) -> Editor<MemorySource> {
    Editor::new(Document::new(MemorySource::new(bytes)), false)
}

#[test]
fn structure_source_and_target_navigation_preserve_document_and_reject_missing_targets() {
    let bytes = fixtures::te64();
    let mut editor = editor(bytes.clone());
    editor.refresh_analysis();
    let image = editor.executable().unwrap();
    let section = image
        .records
        .iter()
        .position(|record| record.kind == StructureKind::Section)
        .unwrap();
    let reference = image.structure_ref(section).unwrap();
    assert_eq!(image.records[section].record.offset.get(), 40);
    editor
        .dispatch(Command::GotoStructureRecord(reference))
        .unwrap();
    assert_eq!(editor.cursor().get(), 40);
    editor.dispatch(Command::GotoStructure(reference)).unwrap();
    assert_eq!(editor.cursor().get(), 0xf08);
    editor.dispatch(Command::GotoEntry).unwrap();
    assert_eq!(editor.cursor().get(), 0xf18);
    let image = editor.executable().unwrap();
    let unsupported = image
        .records
        .iter()
        .position(|record| {
            record.kind == StructureKind::Relocation
                && record.status == StructureStatus::Unsupported
        })
        .unwrap();
    let reference = image.structure_ref(unsupported).unwrap();
    assert_eq!(image.records[unsupported].record.offset.get(), 0x1f12);
    let cursor = editor.cursor();
    let context = editor.decode_context();
    assert!(editor.dispatch(Command::GotoStructure(reference)).is_err());
    assert_eq!(editor.cursor(), cursor);
    assert_eq!(editor.decode_context(), context);
    editor
        .dispatch(Command::GotoStructureRecord(reference))
        .unwrap();
    assert_eq!(editor.cursor().get(), 0x1f12);
    let mut invalid = reference;
    invalid.index = usize::MAX;
    assert!(
        editor
            .dispatch(Command::GotoStructureRecord(invalid))
            .is_err()
    );
    assert_eq!(editor.cursor().get(), 0x1f12);
    let mut actual = vec![0; bytes.len()];
    editor
        .document_mut()
        .read_at(FileOffset::new(0), &mut actual)
        .unwrap();
    assert_eq!(actual, bytes);
    assert!(!editor.document().is_dirty());
}

#[test]
fn edited_mapping_rejects_old_selection_and_fresh_navigation_uses_logical_bytes() {
    let mut editor = editor(fixtures::te32());
    editor.refresh_analysis();
    let image = editor.executable().unwrap();
    let index = image
        .records
        .iter()
        .position(|record| record.kind == StructureKind::Section)
        .unwrap();
    let original = image.structure_ref(index).unwrap();
    editor.dispatch(Command::BeginEdit).unwrap();
    editor
        .document_mut()
        .overwrite(FileOffset::new(60), &0x1008_u32.to_le_bytes())
        .unwrap();
    assert!(editor.executable().is_none());
    let cursor = editor.cursor();
    assert!(
        editor
            .dispatch(Command::GotoStructure(original))
            .unwrap_err()
            .to_string()
            .contains("stale")
    );
    assert_eq!(editor.cursor(), cursor);
    let fresh = editor.executable().unwrap().structure_ref(index).unwrap();
    editor.dispatch(Command::GotoStructure(fresh)).unwrap();
    assert_eq!(editor.cursor().get(), 0xf10);
    editor.dispatch(Command::GotoEntry).unwrap();
    assert_eq!(editor.cursor().get(), 0xf20);
    editor.dispatch(Command::Undo).unwrap();
    assert!(editor.dispatch(Command::GotoStructure(fresh)).is_err());
    editor.dispatch(Command::GotoEntry).unwrap();
    assert_eq!(editor.cursor().get(), 0xf18);
    editor.dispatch(Command::Redo).unwrap();
    editor.dispatch(Command::GotoEntry).unwrap();
    assert_eq!(editor.cursor().get(), 0xf20);
    editor.dispatch(Command::DiscardEdit).unwrap();
    editor.dispatch(Command::GotoEntry).unwrap();
    assert_eq!(editor.cursor().get(), 0xf18);
    assert!(!editor.document().is_dirty());
}

#[test]
fn switching_fat_member_invalidates_structure_selection_at_the_same_generation() {
    let mut editor = editor(macho_fixtures::fat_records());
    editor.refresh_analysis();
    let selection = editor.executable().unwrap().structure_ref(0).unwrap();
    editor
        .dispatch(Command::GotoStructureRecord(selection))
        .unwrap();
    assert_eq!(editor.cursor().get(), 0x11c);
    let generation = editor.document().generation();
    editor.dispatch(Command::SelectSlice(1)).unwrap();
    assert_eq!(editor.document().generation(), generation);
    let cursor = editor.cursor();
    assert!(
        editor
            .dispatch(Command::GotoStructureRecord(selection))
            .unwrap_err()
            .to_string()
            .contains("stale")
    );
    assert_eq!(editor.cursor(), cursor);
    let current = editor.executable().unwrap().structure_ref(0).unwrap();
    editor
        .dispatch(Command::GotoStructureRecord(current))
        .unwrap();
    assert_eq!(editor.cursor().get(), 0x21c);
    assert!(!editor.document().is_dirty());
}

#[test]
fn declared_zero_fill_entry_is_visible_but_rejected_before_navigation() {
    let mut bytes = fixtures::te32();
    fixtures::put32(&mut bytes, 8, 0x2200);
    let mut editor = editor(bytes.clone());
    editor.dispatch(Command::Goto(FileOffset::new(40))).unwrap();
    editor.refresh_analysis();
    assert_eq!(editor.executable().unwrap().entry.unwrap().get(), 0x402200);
    let context = editor.decode_context();
    assert!(
        editor
            .dispatch(Command::GotoEntry)
            .unwrap_err()
            .to_string()
            .contains("zero-fill")
    );
    assert_eq!(editor.cursor().get(), 40);
    assert_eq!(editor.decode_context(), context);
    assert!(editor.dispatch(Command::ReturnBranch).is_err());
    let mut actual = vec![0; bytes.len()];
    editor
        .document_mut()
        .read_at(FileOffset::new(0), &mut actual)
        .unwrap();
    assert_eq!(actual, bytes);
    assert!(!editor.document().is_dirty());
}

#[test]
fn mixed_arm_thumb_te_entry_selects_mode_and_normalizes_only_the_state_bit() {
    use crate::instructions::DecodeMode;
    for (bytes, expected, raw_entry) in [
        (fixtures::te(0x1c2), DecodeMode::Arm, 0x401010),
        (fixtures::te_mixed_thumb(), DecodeMode::Thumb, 0x401011),
    ] {
        let mut editor = editor(bytes);
        editor.dispatch(Command::GotoEntry).unwrap();
        assert_eq!(editor.cursor().get(), 0xf18);
        assert_eq!(editor.executable().unwrap().entry.unwrap().get(), raw_entry);
        assert_eq!(editor.decode_context().mode, Some(expected));
        assert_eq!(editor.code_rows(1).unwrap()[0].offset.get(), 0xf18);
    }
}
