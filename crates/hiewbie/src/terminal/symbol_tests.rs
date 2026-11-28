use super::*;

#[path = "../../../../tests/fixtures/symbol_images.rs"]
mod fixtures;

#[test]
fn header_import_export_and_metadata_keys_follow_exact_bytes() {
    let original = fixtures::pe64();
    let mut ui = ui(&original);
    key(&mut ui, KeyCode::F(8));
    assert!(
        ui.editor.symbols().is_none(),
        "headers must not read symbol tables"
    );
    key(&mut ui, KeyCode::F(7));
    let output = screen(&mut ui, 80, 25);
    assert!(output.contains("Parsed imports"));
    assert!(output.contains("slot VA 140001160 demo.dll!external"));
    assert!(output.contains("Record FO 340+8"));
    key(&mut ui, KeyCode::Enter);
    assert!(ui.dialog.is_none());
    assert_eq!(ui.editor.cursor().get(), 0x360);
    key(&mut ui, KeyCode::F(8));
    key(&mut ui, KeyCode::F(9));
    assert!(screen(&mut ui, 80, 25).contains("entry_symbol"));
    key(&mut ui, KeyCode::Enter);
    assert_eq!(ui.editor.cursor().get(), 0x200);
    key(&mut ui, KeyCode::F(8));
    key(&mut ui, KeyCode::F(9));
    key(&mut ui, KeyCode::Down);
    key(&mut ui, KeyCode::Enter);
    assert!(ui.dialog.is_some());
    assert_eq!(ui.editor.cursor().get(), 0x200);
    assert!(ui.message.contains("no file-backed target"));
    key(&mut ui, KeyCode::Char('r'));
    assert!(ui.dialog.is_none());
    assert_eq!(ui.editor.cursor().get(), 0x444);
    key(&mut ui, KeyCode::F(8));
    key(&mut ui, KeyCode::Char('n'));
    assert!(screen(&mut ui, 80, 25).contains("3 parsed entries"));
    key(&mut ui, KeyCode::F(7));
    key(&mut ui, KeyCode::Char('r'));
    assert_eq!(ui.editor.cursor().get(), 0x340);
    assert_eq!(bytes(&mut ui), original);
}

#[test]
fn symbol_dialog_refreshes_saved_names_and_escapes_untrusted_text() {
    let mut original = fixtures::pe64();
    original[0x460] = 0x1b;
    let mut ui = ui(&original);
    key(&mut ui, KeyCode::F(8));
    key(&mut ui, KeyCode::F(9));
    let output = screen(&mut ui, 80, 25);
    assert!(output.contains(&escaped_text("\x1bntry_symbol")));
    assert!(!output.contains('\x1b'));
    assert!(screen(&mut ui, 80, 9).contains(&escaped_text("\x1bntry_symbol")));
    for (width, height) in [(30, 10), (1, 1), (120, 40)] {
        let output = screen(&mut ui, width, height);
        assert!(!output.contains('\x1b'));
    }
    key(&mut ui, KeyCode::Esc);
    original[0x460] = b'E';
    ui.editor.complete_save(MemorySource::new(original));
    key(&mut ui, KeyCode::F(8));
    key(&mut ui, KeyCode::F(9));
    assert!(screen(&mut ui, 80, 25).contains("Entry_symbol"));
    assert!(!ui.editor.document().is_dirty());
}

#[test]
fn symbol_errors_do_not_disable_header_or_code_navigation() {
    let mut original = fixtures::pe64();
    fixtures::put32(&mut original, 0x300, u32::MAX);
    let mut ui = ui(&original);
    key(&mut ui, KeyCode::F(8));
    key(&mut ui, KeyCode::F(7));
    assert!(screen(&mut ui, 80, 25).contains("symbol inspection unavailable"));
    key(&mut ui, KeyCode::F(8));
    key(&mut ui, KeyCode::F(5));
    assert_eq!(ui.editor.cursor().get(), 0x200);
    assert!(ui.dialog.is_none());
    assert_eq!(bytes(&mut ui), original);
}
