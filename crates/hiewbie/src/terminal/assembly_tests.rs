use super::*;

fn code_editor(bytes: &[u8], mode: DecodeMode) -> Ui<MemorySource> {
    let mut ui = ui(bytes);
    ui.editor.dispatch(Command::SetRaw(true)).unwrap();
    ui.editor
        .dispatch(Command::SetDecodeMode(Some(mode)))
        .unwrap();
    ui.editor
        .dispatch(Command::SetMode(ViewMode::Code))
        .unwrap();
    ui
}

fn source(ui: &mut Ui<MemorySource>, text: &str) {
    control(ui, KeyCode::Char('u'));
    for character in text.chars() {
        key(ui, KeyCode::Char(character));
    }
}

fn segment(frame: &str, row: usize, start: usize, count: usize) -> String {
    frame
        .lines()
        .nth(row)
        .unwrap()
        .chars()
        .skip(start)
        .take(count)
        .collect()
}

#[test]
fn assembler_matches_observed_prompt_geometry_caret_and_candidate_rows() {
    use ratatui::backend::Backend;
    use ratatui::layout::Position;
    use ratatui::style::Color;

    let mut ui = code_editor(&[0x90; 64], DecodeMode::X86_32);
    key(&mut ui, KeyCode::F(3));
    key(&mut ui, KeyCode::F(2));
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    terminal.draw(|frame| ui.render(frame)).unwrap();
    assert!(ui.has_input_caret());
    assert_eq!(
        terminal.backend_mut().get_cursor_position().unwrap(),
        Position::new(21, 15)
    );
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(16, 14)].symbol(), "╔");
    assert_eq!(buffer[(83, 16)].symbol(), "╝");
    assert_eq!(buffer[(86, 15)].bg, Color::Black);
    assert_eq!(buffer[(18, 15)].symbol(), "n");
    assert_eq!(buffer[(18, 15)].bg, Color::Cyan);
    let frame = screen(&mut ui, 100, 30);
    assert_eq!(segment(&frame, 13, 83, 3), "[x]");
    assert_eq!(
        segment(&frame, 16, 16, 68),
        format!("╚{} CommandSelect: Off {}╝", "═".repeat(23), "═".repeat(23))
    );
    assert!(frame.lines().last().unwrap().contains("4Select"));

    key(&mut ui, KeyCode::F(4));
    key(&mut ui, KeyCode::Enter);
    assert!(!ui.has_input_caret());
    terminal.draw(|frame| ui.render(frame)).unwrap();
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(33, 10)].symbol(), "╔");
    assert_eq!(buffer[(66, 20)].symbol(), "╝");
    assert_eq!(buffer[(35, 11)].bg, Color::Black);
    assert_eq!(buffer[(35, 12)].fg, Color::Blue);
    let frame = screen(&mut ui, 100, 30);
    assert_eq!(segment(&frame, 11, 35, 2), "90");
    assert_eq!(segment(&frame, 19, 35, 18), "660F1F840000000000");
    key(&mut ui, KeyCode::Down);
    key(&mut ui, KeyCode::Esc);
    assert!(screen(&mut ui, 100, 30).contains("Canceled"));
    key(&mut ui, KeyCode::Esc);
    assert!(screen(&mut ui, 100, 30).contains("CommandSelect: On"));
    assert_eq!(ui.editor.cursor(), FileOffset::new(0));
    assert!(!ui.editor.document().is_dirty());
    key(&mut ui, KeyCode::Esc);
    assert!(ui.editor.is_editing());
}

#[test]
fn code_assembly_keeps_edit_state_across_help_invalid_input_and_nested_escape() {
    let mut ui = code_editor(&[0xb8, 1, 2, 3, 4, 0xcc], DecodeMode::X86_32);
    key(&mut ui, KeyCode::F(3));
    key(&mut ui, KeyCode::Char('a'));
    key(&mut ui, KeyCode::Enter);
    assert_eq!(ui.pending_nibble, Some(0xa));
    key(&mut ui, KeyCode::F(1));
    assert!(screen(&mut ui, 80, 25).contains("[Assembler]"));
    assert!(!ui.has_input_caret());
    key(&mut ui, KeyCode::Esc);
    assert!(ui.has_input_caret());

    source(&mut ui, "badopcode");
    key(&mut ui, KeyCode::Enter);
    assert!(!ui.has_input_caret());
    assert_eq!(
        segment(&screen(&mut ui, 100, 30), 15, 40, 19),
        "Illegal instruction"
    );
    key(&mut ui, KeyCode::Esc);
    assert!(screen(&mut ui, 80, 25).contains("badopcode"));
    assert!(ui.has_input_caret());
    key(&mut ui, KeyCode::Esc);
    assert_eq!(ui.pending_nibble, Some(0xa));
    assert!(!ui.editor.document().is_dirty());
    assert_eq!(ui.editor.cursor(), FileOffset::new(0));

    key(&mut ui, KeyCode::F(2));
    source(&mut ui, "nop");
    key(&mut ui, KeyCode::Enter);
    assert_eq!(bytes(&mut ui), [0x90, 1, 2, 3, 4, 0xcc]);
    assert_eq!(ui.editor.cursor(), FileOffset::new(1));
    assert!(ui.pending_nibble.is_none());
    assert!(ui.has_input_caret());
    key(&mut ui, KeyCode::Esc);
    control(&mut ui, KeyCode::Char('z'));
    assert_eq!(bytes(&mut ui), [0xb8, 1, 2, 3, 4, 0xcc]);
    control(&mut ui, KeyCode::Char('y'));
    assert_eq!(bytes(&mut ui), [0x90, 1, 2, 3, 4, 0xcc]);
    assert_eq!(key(&mut ui, KeyCode::F(9)), UiAction::Save);
}

#[test]
fn assembler_requires_explicit_widening_and_applies_the_displayed_patch_once() {
    let original = [0xcc; 8];
    let mut ui = code_editor(&original, DecodeMode::X86_32);
    key(&mut ui, KeyCode::F(3));
    key(&mut ui, KeyCode::F(2));
    source(&mut ui, "jmp 10");
    key(&mut ui, KeyCode::Enter);
    assert!(screen(&mut ui, 100, 30).contains("Encoding needs 5 bytes"));
    assert_eq!(bytes(&mut ui), original);
    key(&mut ui, KeyCode::Esc);
    control(&mut ui, KeyCode::Char('p'));
    key(&mut ui, KeyCode::Enter);
    assert!(screen(&mut ui, 100, 30).contains("Patch preview"));
    assert_eq!(bytes(&mut ui), original);
    key(&mut ui, KeyCode::Char('w'));
    key(&mut ui, KeyCode::Char('+'));
    key(&mut ui, KeyCode::Char('p'));
    let preview = screen(&mut ui, 100, 30);
    assert!(preview.contains("x86-32  0  6 byte(s)  NOP"));
    assert!(preview.contains("Before  CC CC CC CC CC CC"));
    assert!(preview.contains("After   E9 0B 00 00 00 90"));
    assert_eq!(bytes(&mut ui), original);
    key(&mut ui, KeyCode::Enter);
    assert_eq!(bytes(&mut ui), [0xe9, 0x0b, 0, 0, 0, 0x90, 0xcc, 0xcc]);
    assert_eq!(ui.editor.cursor(), FileOffset::new(6));
    key(&mut ui, KeyCode::Esc);
    control(&mut ui, KeyCode::Char('z'));
    assert_eq!(bytes(&mut ui), original);
    assert!(!ui.editor.document_mut().undo().unwrap());
}

#[test]
fn assembler_uses_aarch64_actual_address_and_alt_f2_preserves_marked_block() {
    let original = [0x1f, 0x20, 0x03, 0xd5, 0xc0, 0x03, 0x5f, 0xd6];
    let mut ui = code_editor(&original, DecodeMode::Aarch64);
    ui.editor
        .dispatch(Command::SetRawBase(VirtualAddress::new(0x1000)))
        .unwrap();
    key(&mut ui, KeyCode::F(3));
    key(&mut ui, KeyCode::F(2));
    source(&mut ui, "b 1010");
    key(&mut ui, KeyCode::Enter);
    key(&mut ui, KeyCode::Esc);
    assert_eq!(bytes(&mut ui), [4, 0, 0, 0x14, 0xc0, 3, 0x5f, 0xd6]);
    assert_eq!(ui.editor.cursor(), FileOffset::new(4));
    ui.key(KeyEvent::new(KeyCode::F(2), KeyModifiers::ALT));
    assert_eq!(bytes(&mut ui), [4, 0, 0, 0x14, 0x1f, 0x20, 3, 0xd5]);
    control(&mut ui, KeyCode::Char('z'));
    assert_eq!(bytes(&mut ui), [4, 0, 0, 0x14, 0xc0, 3, 0x5f, 0xd6]);
    key(&mut ui, KeyCode::Esc);

    ui.editor.dispatch(Command::SelectWholeBlock).unwrap();
    assert_eq!(key(&mut ui, KeyCode::F(2)), UiAction::Continue);
    assert!(matches!(ui.dialog, Some(Dialog::BlockWrite(_))));
    key(&mut ui, KeyCode::Esc);
    key(&mut ui, KeyCode::F(3));
    ui.key(KeyEvent::new(KeyCode::F(2), KeyModifiers::ALT));
    assert_eq!(bytes(&mut ui), original);
    assert!(!ui.editor.document().is_dirty());
}

#[test]
fn assembler_closes_at_eof_without_reseeding_inside_a64_word() {
    let mut ui = code_editor(&[0xc0, 3, 0x5f, 0xd6], DecodeMode::Aarch64);
    key(&mut ui, KeyCode::F(3));
    key(&mut ui, KeyCode::F(2));
    source(&mut ui, "nop");
    key(&mut ui, KeyCode::Enter);
    assert_eq!(bytes(&mut ui), [0x1f, 0x20, 3, 0xd5]);
    assert!(ui.dialog.is_none());
    assert_eq!(ui.editor.cursor(), FileOffset::new(3));
    assert!(ui.editor.is_editing());
}

#[test]
fn assembler_reseeds_observed_far_forms_without_reencoding_different_bytes() {
    let original = [0xff, 0x28, 0x66, 0xff, 0x28, 0xff, 0x18, 0x90];
    let mut ui = code_editor(&original, DecodeMode::X86_32);
    key(&mut ui, KeyCode::F(3));
    key(&mut ui, KeyCode::F(2));
    for (prefill, next) in [
        ("jmpf         f,[eax]", 2),
        ("jmpf         d,[eax]", 5),
        ("callf        f,[eax]", 7),
    ] {
        assert!(screen(&mut ui, 100, 30).contains(prefill));
        key(&mut ui, KeyCode::Enter);
        assert_eq!(ui.editor.cursor(), FileOffset::new(next));
        assert_eq!(bytes(&mut ui), original);
        assert!(!ui.editor.document().is_dirty());
    }
}
