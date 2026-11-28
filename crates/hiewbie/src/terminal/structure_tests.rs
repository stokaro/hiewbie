use super::*;
use hiewbie_core::formats::{StructureKind, StructureStatus};

#[path = "../../../../tests/fixtures/macho_records.rs"]
#[allow(dead_code)]
mod macho;
#[path = "../../../../tests/fixtures/te_images.rs"]
#[allow(dead_code)]
mod te;

#[test]
fn te_structure_keys_follow_target_or_record_and_unknown_types_remain_visible() {
    let original = te::te64();
    let mut ui = ui(&original);
    key(&mut ui, KeyCode::F(8));
    assert!(screen(&mut ui, 100, 28).contains("Format TE64"));
    key(&mut ui, KeyCode::Char('t'));
    let output = screen(&mut ui, 100, 28);
    assert!(output.contains("Format structure records"));
    assert!(output.contains("Record FO 28+28"));
    assert!(output.contains("Target FO F08+100"));
    key(&mut ui, KeyCode::Enter);
    assert_eq!(ui.editor.cursor().get(), 0xf08);
    assert!(ui.dialog.is_none());
    key(&mut ui, KeyCode::F(8));
    key(&mut ui, KeyCode::Char('t'));
    key(&mut ui, KeyCode::Char('r'));
    assert_eq!(ui.editor.cursor().get(), 40);
    key(&mut ui, KeyCode::F(8));
    key(&mut ui, KeyCode::Char('t'));
    let unknown = ui
        .editor
        .executable()
        .unwrap()
        .records
        .iter()
        .position(|record| {
            record.kind == StructureKind::Relocation
                && record.status == StructureStatus::Unsupported
        })
        .unwrap();
    for _ in 0..unknown {
        key(&mut ui, KeyCode::Down);
    }
    assert!(screen(&mut ui, 100, 28).contains("unsupported"));
    key(&mut ui, KeyCode::Enter);
    assert!(ui.dialog.is_some());
    assert_eq!(ui.editor.cursor().get(), 40);
    assert!(ui.message.contains("no file-backed target"));
    key(&mut ui, KeyCode::Char('r'));
    assert_eq!(ui.editor.cursor().get(), 0x1f12);
    assert_eq!(bytes(&mut ui), original);
    assert!(!ui.editor.document().is_dirty());
}

#[test]
fn structure_list_reaches_last_command_and_handles_tiny_screens_and_detail_scroll() {
    let original = macho::unknown_commands(100);
    let mut ui = ui(&original);
    key(&mut ui, KeyCode::F(8));
    key(&mut ui, KeyCode::Char('t'));
    key(&mut ui, KeyCode::End);
    let output = screen(&mut ui, 100, 28);
    assert!(output.contains("100 records"));
    assert!(output.contains("99 unsupported"));
    assert!(output.contains("Record FO 334+8"));
    key(&mut ui, KeyCode::Right);
    key(&mut ui, KeyCode::Left);
    for (width, height) in [(80, 9), (30, 10), (1, 1), (120, 40)] {
        assert!(!screen(&mut ui, width, height).contains('\x1b'));
    }
    key(&mut ui, KeyCode::Enter);
    assert!(ui.dialog.is_some());
    assert_eq!(ui.editor.cursor().get(), 0);
    key(&mut ui, KeyCode::Char('r'));
    assert_eq!(ui.editor.cursor().get(), 0x334);
    assert_eq!(bytes(&mut ui), original);
}

#[test]
fn malformed_te_explains_raw_fallback_and_saved_fields_refresh_in_dialog() {
    let mut original = te::te32();
    let mut ui = ui(&original);
    key(&mut ui, KeyCode::F(8));
    key(&mut ui, KeyCode::Char('t'));
    key(&mut ui, KeyCode::Esc);
    original[40] = 0x1b;
    ui.editor.complete_save(MemorySource::new(original.clone()));
    key(&mut ui, KeyCode::F(8));
    key(&mut ui, KeyCode::Char('t'));
    let output = screen(&mut ui, 100, 28);
    assert!(output.contains(".text"));
    assert!(!output.contains('\x1b'));
    key(&mut ui, KeyCode::Esc);
    te::put16(&mut original, 6, 39);
    ui.editor.complete_save(MemorySource::new(original.clone()));
    key(&mut ui, KeyCode::F(8));
    key(&mut ui, KeyCode::Char('t'));
    let output = screen(&mut ui, 100, 28);
    assert!(output.contains("structures unavailable"));
    assert!(output.contains("stripped size"));
    key(&mut ui, KeyCode::Esc);
    assert_eq!(bytes(&mut ui), original);
}
