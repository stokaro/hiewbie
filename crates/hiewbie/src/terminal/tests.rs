use std::cell::Cell;
use std::io;
use std::rc::Rc;

use hiewbie_core::instructions::DecodeMode;
use hiewbie_core::{Document, MemorySource};
use ratatui::backend::TestBackend;

use super::search::{SEARCH_STEP_BYTES, parse_hex_pattern};
use super::*;
use hiewbie_core::{MAX_SEARCH_PATTERN_BYTES, SearchRequest};

#[path = "symbol_tests.rs"]
mod symbols;

fn ui(bytes: &[u8]) -> Ui<MemorySource> {
    Ui::new(
        Editor::new(Document::new(MemorySource::new(bytes.to_vec())), false),
        "fixture.bin".into(),
    )
}

fn key<S: ByteSource>(ui: &mut Ui<S>, code: KeyCode) -> UiAction {
    ui.key(KeyEvent::new(code, KeyModifiers::NONE))
}

fn control<S: ByteSource>(ui: &mut Ui<S>, code: KeyCode) -> UiAction {
    ui.key(KeyEvent::new(code, KeyModifiers::CONTROL))
}

fn type_text<S: ByteSource>(ui: &mut Ui<S>, text: &str) {
    for character in text.chars() {
        key(ui, KeyCode::Char(character));
    }
}

fn bytes(ui: &mut Ui<MemorySource>) -> Vec<u8> {
    let mut output = vec![0; ui.editor.document().len() as usize];
    ui.editor
        .document_mut()
        .read_at(FileOffset::new(0), &mut output)
        .unwrap();
    output
}

fn screen<S: ByteSource>(ui: &mut Ui<S>, width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| ui.render(frame)).unwrap();
    terminal
        .backend()
        .buffer()
        .content
        .chunks(usize::from(width).max(1))
        .map(|cells| {
            cells
                .iter()
                .map(|cell| cell.symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn hex_view_has_stable_dense_cells_at_eighty_columns() {
    let mut ui = ui(b"Hello\0world!\n\xffXYZ");
    let output = screen(&mut ui, 80, 8);
    let expected = [
        format!(
            "▲    fixture.bin      ↓FRO --------                  00000000│hiewbie {}",
            env!("CARGO_PKG_VERSION")
        ),
        "↑ 00000000:  48 65 6C 6C-6F 00 77 6F-72 6C 64 21-0A FF 58 59  Hello world!◙\u{a0}XY"
            .into(),
        "▓ 00000010:  5A         -           -           -             Z".into(),
        "▓".into(),
        "▓".into(),
        "↓".into(),
        "▼".into(),
        " 1Help   2PutBlk 3Edit   4Mode   5Goto   6DatRef 7Search 8Header 9Files 10Quit".into(),
    ]
    .join("\n");
    assert_eq!(output, expected);
    assert_eq!(ui.bytes_per_row, 16);
}

#[test]
fn hex_digits_are_atomic_and_motion_cancels_a_partial_byte() {
    let mut ui = ui(&[0x11, 0x22, 0x33]);
    key(&mut ui, KeyCode::F(3));
    key(&mut ui, KeyCode::Char('a'));
    assert_eq!(bytes(&mut ui), [0x11, 0x22, 0x33]);
    assert!(!ui.editor.document().is_dirty());
    assert_eq!(key(&mut ui, KeyCode::F(9)), UiAction::Continue);
    assert!(ui.message.contains("Incomplete hex byte"));
    key(&mut ui, KeyCode::Right);
    assert_eq!(ui.pending_nibble, None);
    type_text(&mut ui, "42");
    assert_eq!(bytes(&mut ui), [0x11, 0x42, 0x33]);
    assert_eq!(ui.editor.cursor(), FileOffset::new(2));
    assert!(ui.editor.document().is_dirty());
    key(&mut ui, KeyCode::F(3));
    assert_eq!(bytes(&mut ui), [0x11, 0x42, 0x33]);
    assert!(ui.editor.is_editing());
    assert_eq!(key(&mut ui, KeyCode::Esc), UiAction::Continue);
    assert!(!ui.editor.is_editing());
    assert_eq!(bytes(&mut ui), [0x11, 0x22, 0x33]);
    assert_eq!(key(&mut ui, KeyCode::Esc), UiAction::Exit);
}

#[test]
fn contextual_save_ascii_undo_and_redo_use_shared_commands() {
    let mut ui = ui(b"ABC");
    assert_eq!(key(&mut ui, KeyCode::F(9)), UiAction::BrowseFiles);
    assert!(!ui.editor.is_editing());
    control(&mut ui, KeyCode::Char('e'));
    key(&mut ui, KeyCode::Tab);
    key(&mut ui, KeyCode::Char('Z'));
    assert_eq!(bytes(&mut ui), b"ZBC");
    control(&mut ui, KeyCode::Char('z'));
    assert_eq!(bytes(&mut ui), b"ABC");
    control(&mut ui, KeyCode::Char('y'));
    assert_eq!(bytes(&mut ui), b"ZBC");
    assert_eq!(control(&mut ui, KeyCode::Char('s')), UiAction::Save);
    assert!(
        ui.editor.is_editing(),
        "adapter must retain transaction until persistence succeeds"
    );
    assert!(ui.editor.document().is_dirty());
}

#[test]
fn save_errors_distinguish_pre_replacement_failure_from_committed_state() {
    let mut ui = ui(b"ABC");
    key(&mut ui, KeyCode::F(3));
    type_text(&mut ui, "44");
    ui.save_failed(SaveError::Unsupported("test refusal"));
    assert!(!ui.reopen_required);
    assert!(ui.editor.is_editing());
    assert_eq!(bytes(&mut ui), b"DBC");
    assert_eq!(key(&mut ui, KeyCode::F(9)), UiAction::Save);

    ui.save_failed(SaveError::ReplacedButRefreshFailed(io::Error::other(
        "test verification failure",
    )));
    assert!(ui.reopen_required);
    assert!(
        ui.editor.document().is_dirty(),
        "retain overlay until explicit exit"
    );
    type_text(&mut ui, "ff");
    assert_eq!(key(&mut ui, KeyCode::F(9)), UiAction::Continue);
    assert_eq!(bytes(&mut ui), b"DBC");
    ui.reject_paste();
    let output = screen(&mut ui, 80, 25);
    assert!(output.contains("File replaced; reopen required"));
    assert!(output.contains("File replaced | Esc exits"));
    assert_eq!(key(&mut ui, KeyCode::Esc), UiAction::Exit);
}

#[test]
fn bookmarks_restore_hex_viewport_and_show_the_active_slot_in_each_title() {
    let mut ui = ui(&[b'A'; 0x700]);
    ui.dispatch(Command::Goto(FileOffset::new(0x221)));
    ui.top = 0x200;
    let original = screen(&mut ui, 80, 25);
    assert!(original.lines().nth(1).unwrap().contains("00000200:"));
    assert_eq!(
        ui.key(KeyEvent::new(KeyCode::Char('+'), KeyModifiers::SHIFT)),
        UiAction::Continue
    );
    ui.dispatch(Command::Goto(FileOffset::new(0)));
    assert!(screen(&mut ui, 80, 25).contains("↓FRO ♦-------                  00000000"));
    ui.key(KeyEvent::new(KeyCode::Char('1'), KeyModifiers::ALT));
    let restored = screen(&mut ui, 80, 25);
    assert!(restored.contains("↓FRO ♦-------                  00000221"));
    assert_eq!(restored.lines().nth(1), original.lines().nth(1));
    assert_eq!(ui.editor.cursor(), FileOffset::new(0x221));

    for mode in [ViewMode::Text, ViewMode::Code] {
        ui.dispatch(Command::SetMode(mode));
        let view = screen(&mut ui, 80, 25);
        assert!(view.lines().next().unwrap().contains("↓FRO --------"));
        key(&mut ui, KeyCode::Char('+'));
        assert!(
            screen(&mut ui, 80, 25)
                .lines()
                .next()
                .unwrap()
                .contains("↓FRO ♦-------")
        );
    }
    ui.key(KeyEvent::new(KeyCode::Char('0'), KeyModifiers::ALT));
    ui.dispatch(Command::SetMode(ViewMode::Hex));
    assert!(screen(&mut ui, 80, 25).contains("↓FRO --------"));
    assert_eq!(bytes(&mut ui), [b'A'; 0x700]);
}

#[test]
fn text_bookmark_restores_horizontal_scroll_under_the_current_wrap_setting() {
    let mut ui = ui(b"0123456789 ABCDEFGHIJ
0123456789 second row
");
    ui.dispatch(Command::SetMode(ViewMode::Text));
    screen(&mut ui, 80, 25);
    if ui.text.horizontal().is_none() {
        key(&mut ui, KeyCode::F(2));
    }
    key(&mut ui, KeyCode::Right);
    key(&mut ui, KeyCode::Right);
    key(&mut ui, KeyCode::Char('+'));
    key(&mut ui, KeyCode::Right);
    key(&mut ui, KeyCode::Right);
    key(&mut ui, KeyCode::Char('-'));
    assert_eq!(ui.text.horizontal(), Some(2));
    assert!(
        screen(&mut ui, 80, 25)
            .lines()
            .nth(1)
            .unwrap()
            .starts_with("↑2345")
    );
    key(&mut ui, KeyCode::F(2));
    key(&mut ui, KeyCode::Char('-'));
    assert_eq!(ui.text.horizontal(), None);
    assert!(
        screen(&mut ui, 80, 25)
            .lines()
            .nth(1)
            .unwrap()
            .starts_with("↑0123")
    );
}

#[test]
fn modes_share_cursor_and_read_only_cannot_begin_editing() {
    let mut ui = ui(b"0123456789");
    key(&mut ui, KeyCode::Right);
    key(&mut ui, KeyCode::Enter);
    assert_eq!(ui.editor.mode(), ViewMode::Code);
    assert_eq!(ui.editor.cursor(), FileOffset::new(1));
    key(&mut ui, KeyCode::F(4));
    assert_eq!(ui.editor.mode(), ViewMode::Code);
    key(&mut ui, KeyCode::Up);
    key(&mut ui, KeyCode::Enter);
    assert_eq!(ui.editor.mode(), ViewMode::Hex);
    assert_eq!(ui.editor.cursor(), FileOffset::new(1));
    let editor = Editor::new(Document::new(MemorySource::new(b"ABC".to_vec())), true);
    let mut read_only = Ui::new(editor, "read-only".into());
    key(&mut read_only, KeyCode::F(3));
    let view = screen(&mut read_only, 80, 25);
    assert!(view.contains("↓FRO --------"));
    assert!(view.contains("the document is open read-only"));
    type_text(&mut read_only, "ff");
    assert!(!read_only.editor.is_editing());
    assert_eq!(bytes(&mut read_only), b"ABC");
}

#[test]
fn byte_restore_keys_preserve_other_edits_and_support_undo_redo() {
    let mut ui = ui(&[0x11, 0x22, 0x33]);
    key(&mut ui, KeyCode::F(3));
    type_text(&mut ui, "AA");
    key(&mut ui, KeyCode::Left);
    type_text(&mut ui, "BBCC");
    key(&mut ui, KeyCode::Home);
    key(&mut ui, KeyCode::Char('f'));
    key(&mut ui, KeyCode::F(3));
    assert_eq!(ui.pending_nibble, None);
    assert_eq!(bytes(&mut ui), [0x11, 0xCC, 0x33]);
    assert_eq!(ui.editor.cursor(), FileOffset::new(0));
    assert!(ui.editor.document().is_dirty());
    control(&mut ui, KeyCode::Char('z'));
    assert_eq!(bytes(&mut ui), [0xBB, 0xCC, 0x33]);
    control(&mut ui, KeyCode::Char('y'));
    assert_eq!(bytes(&mut ui), [0x11, 0xCC, 0x33]);

    key(&mut ui, KeyCode::Right);
    key(&mut ui, KeyCode::Right);
    key(&mut ui, KeyCode::Char('a'));
    key(&mut ui, KeyCode::Backspace);
    assert_eq!(ui.pending_nibble, None);
    assert_eq!(bytes(&mut ui), [0x11, 0xCC, 0x33]);
    assert_eq!(ui.editor.cursor(), FileOffset::new(2));
    key(&mut ui, KeyCode::Backspace);
    assert_eq!(bytes(&mut ui), [0x11, 0x22, 0x33]);
    assert_eq!(ui.editor.cursor(), FileOffset::new(1));
    assert!(!ui.editor.document().is_dirty());
    assert!(ui.editor.is_editing());
    control(&mut ui, KeyCode::Char('z'));
    assert_eq!(bytes(&mut ui), [0x11, 0xCC, 0x33]);
    control(&mut ui, KeyCode::Char('y'));
    assert_eq!(bytes(&mut ui), [0x11, 0x22, 0x33]);
    key(&mut ui, KeyCode::Home);
    key(&mut ui, KeyCode::Backspace);
    assert_eq!(ui.editor.cursor(), FileOffset::new(0));
    assert_eq!(bytes(&mut ui), [0x11, 0x22, 0x33]);
}

#[test]
fn mode_picker_cancels_and_selects_all_views_without_moving_or_editing() {
    let mut ui = ui(b"0123456789");
    key(&mut ui, KeyCode::Right);
    key(&mut ui, KeyCode::F(4));
    assert!(matches!(ui.dialog, Some(Dialog::Mode(_))));
    assert_eq!(ui.editor.mode(), ViewMode::Hex);
    assert_eq!(ui.editor.cursor(), FileOffset::new(1));
    let output = screen(&mut ui, 80, 25);
    let popup = output
        .lines()
        .skip(10)
        .take(5)
        .map(|line| line.chars().skip(27).take(25).collect::<String>())
        .collect::<Vec<_>>();
    assert_eq!(
        popup,
        [
            "╔═════ Select mode ═════╗".into(),
            "║         Text          ║".into(),
            "║         Hex           ║".into(),
            "║         Decode        ║".into(),
            format!("╚{}╝", "═".repeat(23)),
        ]
    );
    assert_eq!(key(&mut ui, KeyCode::Esc), UiAction::Continue);
    assert!(ui.dialog.is_none());
    assert_eq!(ui.editor.mode(), ViewMode::Hex);

    key(&mut ui, KeyCode::F(4));
    key(&mut ui, KeyCode::Down);
    key(&mut ui, KeyCode::Enter);
    assert!(ui.dialog.is_none());
    assert_eq!(ui.editor.mode(), ViewMode::Code);
    assert_eq!(ui.editor.cursor(), FileOffset::new(1));
    assert!(
        screen(&mut ui, 80, 25)
            .lines()
            .next()
            .unwrap()
            .contains("a32")
    );
    key(&mut ui, KeyCode::F(4));
    assert!(matches!(ui.dialog, Some(Dialog::Mode(ModeChoice::Code))));
    key(&mut ui, KeyCode::Up);
    key(&mut ui, KeyCode::Up);
    key(&mut ui, KeyCode::Enter);
    assert!(ui.dialog.is_none());
    assert_eq!(ui.editor.mode(), ViewMode::Text);
    assert_eq!(ui.editor.cursor(), FileOffset::new(1));
    key(&mut ui, KeyCode::Enter);
    assert_eq!(ui.editor.mode(), ViewMode::Hex);
    key(&mut ui, KeyCode::Enter);
    assert_eq!(ui.editor.mode(), ViewMode::Code);
    key(&mut ui, KeyCode::Enter);
    assert_eq!(ui.editor.mode(), ViewMode::Text);
    key(&mut ui, KeyCode::Enter);
    assert_eq!(ui.editor.mode(), ViewMode::Hex);

    key(&mut ui, KeyCode::F(4));
    for (width, height) in [(23, 5), (40, 10), (80, 25)] {
        let output = screen(&mut ui, width, height);
        assert!(output.contains("Select mode"));
        assert!(output.contains("Hex"));
        assert!(!output.contains("unavailable"));
    }
    key(&mut ui, KeyCode::Enter);
    assert!(ui.dialog.is_none());
    assert_eq!(ui.editor.mode(), ViewMode::Hex);
    assert_eq!(ui.editor.cursor(), FileOffset::new(1));
    assert_eq!(bytes(&mut ui), b"0123456789");
    assert!(!ui.editor.document().is_dirty());
    key(&mut ui, KeyCode::F(3));
    key(&mut ui, KeyCode::F(4));
    assert!(ui.dialog.is_none());
    assert!(ui.editor.is_editing());
    assert_eq!(ui.editor.mode(), ViewMode::Hex);
}

#[test]
fn help_and_paste_cannot_execute_or_discard_edits() {
    let mut ui = ui(b"ABC");
    key(&mut ui, KeyCode::F(3));
    type_text(&mut ui, "44");
    key(&mut ui, KeyCode::F(1));
    assert!(screen(&mut ui, 80, 25).contains("[HexEdit]"));
    key(&mut ui, KeyCode::Esc);
    assert!(ui.editor.is_editing());
    assert_eq!(bytes(&mut ui), b"DBC");
    key(&mut ui, KeyCode::Char('f'));
    ui.reject_paste();
    assert_eq!(ui.pending_nibble, None);
    assert!(ui.editor.is_editing());
    assert_eq!(bytes(&mut ui), b"DBC");
    assert!(ui.message.contains("Paste rejected"));
}

#[test]
fn help_matches_the_hex_console_geometry_and_pages_without_dispatching_commands() {
    use ratatui::style::Color;

    let mut ui = ui(b"ABCD");
    key(&mut ui, KeyCode::Right);
    key(&mut ui, KeyCode::F(1));

    let mut terminal = Terminal::new(TestBackend::new(80, 25)).unwrap();
    terminal.draw(|frame| ui.render(frame)).unwrap();
    let cells = terminal.backend().buffer();
    assert_eq!(cells[(4, 2)].bg, Color::Green);
    assert_eq!(cells[(73, 2)].symbol(), "[");
    assert_eq!(cells[(6, 3)].symbol(), "╔");
    assert_eq!(cells[(6, 3)].fg, Color::Black);
    assert_eq!(cells[(73, 3)].symbol(), "╗");
    assert_eq!(cells[(37, 3)].symbol(), "[");
    assert_eq!(cells[(9, 4)].symbol(), "E");
    assert_eq!(cells[(27, 4)].symbol(), "-");
    assert_eq!(cells[(7, 4)].bg, Color::Green);
    assert_eq!(cells[(8, 4)].bg, Color::Black);
    assert_eq!(cells[(71, 4)].bg, Color::Black);
    assert_eq!(cells[(72, 4)].bg, Color::Green);
    assert_eq!(cells[(9, 4)].fg, Color::White);
    assert_eq!(cells[(9, 5)].fg, Color::LightYellow);
    assert_eq!(cells[(6, 21)].symbol(), "╚");
    assert_eq!(cells[(77, 23)].bg, Color::Black);
    assert_eq!(cells[(2, 24)].symbol(), " ");
    assert_eq!(cells[(2, 24)].bg, Color::Cyan);

    let initial = screen(&mut ui, 80, 25);
    assert!(
        initial
            .lines()
            .nth(20)
            .unwrap()
            .contains("F8       - Header")
    );
    key(&mut ui, KeyCode::F(1));
    assert_eq!(screen(&mut ui, 80, 25), initial);
    key(&mut ui, KeyCode::Down);
    key(&mut ui, KeyCode::PageDown);
    let paged = screen(&mut ui, 80, 25);
    assert!(paged.lines().nth(4).unwrap().contains("F9       - Files"));
    assert!(paged.lines().nth(20).unwrap().contains("Ctrl-F5  - Base"));
    key(&mut ui, KeyCode::PageUp);
    assert_eq!(screen(&mut ui, 80, 25), initial);
    key(&mut ui, KeyCode::Down);
    key(&mut ui, KeyCode::Enter);
    assert!(ui.dialog.is_none());
    assert_eq!(ui.editor.cursor(), FileOffset::new(1));
    assert_eq!(ui.editor.mode(), ViewMode::Hex);
    assert_eq!(bytes(&mut ui), b"ABCD");
    assert!(!ui.editor.document().is_dirty());
}

#[test]
fn help_restores_the_pending_edit_caret_and_keeps_search_modal() {
    let mut ui = ui(b"ABCD");
    key(&mut ui, KeyCode::F(3));
    key(&mut ui, KeyCode::Char('A'));
    assert_eq!(ui.pending_nibble, Some(10));
    assert_eq!(bytes(&mut ui), b"ABCD");
    key(&mut ui, KeyCode::F(1));
    assert!(!ui.has_input_caret());
    assert!(screen(&mut ui, 132, 50).contains("[HexEdit]"));
    key(&mut ui, KeyCode::End);
    assert!(screen(&mut ui, 55, 12).contains("Backspace"));
    key(&mut ui, KeyCode::Enter);
    assert!(ui.has_input_caret());
    assert_eq!(ui.pending_nibble, Some(10));
    assert_eq!(ui.editor.cursor(), FileOffset::new(0));
    key(&mut ui, KeyCode::Esc);

    key(&mut ui, KeyCode::F(7));
    type_text(&mut ui, "41 42");
    let prompt = screen(&mut ui, 80, 25);
    key(&mut ui, KeyCode::F(1));
    assert!(!ui.has_input_caret());
    let help = screen(&mut ui, 80, 25);
    assert!(help.lines().nth(4).unwrap().contains("[FindEdit]"));
    assert!(help.lines().nth(5).unwrap().contains("F2       - Direct"));
    assert!(help.lines().nth(19).unwrap().contains("Shift-Insert"));
    key(&mut ui, KeyCode::F(2));
    key(&mut ui, KeyCode::F(1));
    assert_eq!(screen(&mut ui, 80, 25), help);
    key(&mut ui, KeyCode::Enter);
    assert_eq!(screen(&mut ui, 80, 25), prompt);
    assert!(ui.has_input_caret());
    assert!(!ui.editor.is_searching());
    assert!(!ui.editor.document().is_dirty());
    assert_eq!(bytes(&mut ui), b"ABCD");
}

#[test]
fn code_help_keeps_distinct_view_and_edit_entries_on_wide_consoles() {
    let mut ui = ui(&[0x90; 64]);
    ui.dispatch(Command::SetMode(ViewMode::Code));
    key(&mut ui, KeyCode::F(1));
    let wide = screen(&mut ui, 132, 50);
    assert!(wide.lines().nth(3).unwrap().contains("[Code]"));
    assert!(wide.lines().nth(21).unwrap().contains("F6       - Refer"));
    assert!(wide.lines().nth(45).unwrap().contains("Ctrl-F7  - Other"));
    assert!(wide.lines().last().unwrap().ends_with(&"▒".repeat(36)));
    key(&mut ui, KeyCode::Esc);
    key(&mut ui, KeyCode::F(3));
    key(&mut ui, KeyCode::F(1));
    let edit = screen(&mut ui, 80, 25);
    assert!(edit.lines().nth(4).unwrap().contains("[CodeEdit]"));
    assert!(edit.lines().nth(5).unwrap().contains("Discard"));
    assert!(edit.lines().nth(8).unwrap().contains("F2       - Asm"));
    key(&mut ui, KeyCode::Esc);
    assert!(ui.editor.is_editing());
    assert_eq!(bytes(&mut ui), [0x90; 64]);
}

#[test]
fn goto_dialog_validates_and_preserves_context_on_error() {
    let mut ui = ui(&[0; 64]);
    key(&mut ui, KeyCode::F(5));
    type_text(&mut ui, "20t");
    key(&mut ui, KeyCode::Enter);
    assert!(ui.dialog.is_none());
    assert_eq!(ui.editor.cursor(), FileOffset::new(20));
    control(&mut ui, KeyCode::Char('g'));
    type_text(&mut ui, "+10");
    key(&mut ui, KeyCode::Enter);
    assert_eq!(ui.editor.cursor(), FileOffset::new(36));
    key(&mut ui, KeyCode::F(5));
    type_text(&mut ui, "FFFFFFFFFFFFFFFFF");
    key(&mut ui, KeyCode::Enter);
    assert!(matches!(
        ui.dialog,
        Some(Dialog::Input(InputDialog { error: Some(_), .. }))
    ));
    assert_eq!(ui.editor.cursor(), FileOffset::new(36));
    assert_eq!(key(&mut ui, KeyCode::Esc), UiAction::Continue);
    assert!(ui.dialog.is_none());
}

#[test]
fn literal_search_repeats_and_long_scan_can_be_canceled() {
    let mut ui = ui(b"a--a--a");
    key(&mut ui, KeyCode::F(7));
    type_text(&mut ui, "61");
    key(&mut ui, KeyCode::Enter);
    ui.advance_search();
    assert_eq!(ui.editor.cursor(), FileOffset::new(0));
    control(&mut ui, KeyCode::Enter);
    ui.advance_search();
    assert_eq!(ui.editor.cursor(), FileOffset::new(3));
    control(&mut ui, KeyCode::Char('r'));
    ui.advance_search();
    assert_eq!(ui.editor.cursor(), FileOffset::new(6));
    control(&mut ui, KeyCode::Char('r'));
    assert!(!ui.editor.is_searching());
    assert!(ui.message.contains("No further match"));

    let mut long = self::ui(&vec![0; SEARCH_STEP_BYTES * 3]);
    long.editor
        .start_search(SearchRequest::bytes(vec![1, 2, 3]).unwrap());
    long.advance_search();
    assert!(long.editor.is_searching());
    let processing = screen(&mut long, 80, 25);
    assert!(processing.contains("Processing..."));
    assert!(processing.lines().next().unwrap().contains("▓░"));
    assert_eq!(key(&mut long, KeyCode::Esc), UiAction::Continue);
    assert!(!long.editor.is_searching());
    assert_eq!(long.editor.cursor(), FileOffset::new(0));
    assert!(long.message.contains("canceled"));
    assert!(matches!(
        long.dialog,
        Some(Dialog::SearchNotice("Aborted by hacker"))
    ));
    assert!(screen(&mut long, 80, 25).contains("Aborted by hacker"));
    key(&mut long, KeyCode::Enter);
    assert!(long.dialog.is_none());
}

#[test]
fn shift_f7_repeats_search_only_from_view_without_modifier_collisions() {
    let mut ui = ui(b"a--a--a");
    let repeat = KeyEvent::new(KeyCode::F(7), KeyModifiers::SHIFT);
    ui.key(repeat);
    assert!(ui.message.contains("No previous search"));
    assert!(ui.dialog.is_none());
    key(&mut ui, KeyCode::F(7));
    type_text(&mut ui, "61");
    ui.key(repeat);
    assert!(matches!(ui.dialog, Some(Dialog::Search(_))));
    assert!(!ui.editor.is_searching());
    key(&mut ui, KeyCode::Enter);
    ui.advance_search();
    assert_eq!(ui.editor.cursor(), FileOffset::new(0));
    ui.key(repeat);
    ui.advance_search();
    assert_eq!(ui.editor.cursor(), FileOffset::new(3));
    ui.key(KeyEvent::new(
        KeyCode::F(7),
        KeyModifiers::SHIFT | KeyModifiers::ALT,
    ));
    assert!(!ui.editor.is_searching());
    assert_eq!(ui.editor.cursor(), FileOffset::new(3));
    key(&mut ui, KeyCode::F(3));
    ui.key(repeat);
    assert!(!ui.editor.is_searching());
    assert!(ui.editor.is_editing());
    assert_eq!(ui.editor.cursor(), FileOffset::new(3));
    assert_eq!(bytes(&mut ui), b"a--a--a");
    assert!(!ui.editor.document().is_dirty());
}

#[test]
fn x86_references_switch_hex_to_decode_and_share_the_search_dialog() {
    let mut fixture = [0xcc; 0x40];
    fixture[..2].copy_from_slice(&[0xeb, 0x1e]);
    fixture[8..13].copy_from_slice(&[0xe8, 0x13, 0, 0, 0]);
    let mut ui = ui(&fixture);
    ui.dispatch(Command::Goto(FileOffset::new(0x20)));
    assert!(screen(&mut ui, 80, 25).contains("6DatRef"));
    key(&mut ui, KeyCode::F(6));
    assert!(screen(&mut ui, 80, 25).contains("Processing..."));
    assert!(!ui.has_input_caret());
    ui.advance_search();
    assert_eq!(ui.editor.mode(), ViewMode::Code);
    assert_eq!(ui.editor.cursor().get(), 0);
    let first = screen(&mut ui, 80, 25);
    assert!(first.contains("6Refer"));
    assert!(first.contains("00000000: EB1E"));
    control(&mut ui, KeyCode::F(6));
    ui.advance_search();
    assert_eq!(ui.editor.cursor().get(), 8);
    control(&mut ui, KeyCode::Char('t'));
    ui.advance_search();
    let missed = screen(&mut ui, 80, 25);
    assert!(missed.contains("Target not found"));
    assert!(missed.contains("╔═"));
    assert!(missed.lines().next().unwrap().contains("0000003F│▓"));
    assert_eq!(ui.editor.cursor().get(), 8);
    control(&mut ui, KeyCode::F(6));
    assert!(ui.dialog.is_none());
    assert!(!ui.editor.is_searching());
    key(&mut ui, KeyCode::Backspace);
    assert_eq!(ui.editor.cursor().get(), 8);
    key(&mut ui, KeyCode::Char('0'));
    assert_eq!(ui.editor.cursor().get(), 0);
    key(&mut ui, KeyCode::Char('0'));
    assert_eq!(ui.editor.cursor().get(), 8);
    key(&mut ui, KeyCode::F(3));
    key(&mut ui, KeyCode::F(6));
    assert!(ui.editor.is_editing());
    assert!(!ui.editor.is_searching());
    assert_eq!(bytes(&mut ui), fixture);
}

#[test]
fn long_reference_scan_progresses_in_full_size_steps_and_can_be_canceled() {
    let mut ui = ui(&vec![0xcc; SEARCH_STEP_BYTES * 3]);
    let start = FileOffset::new(0x20);
    ui.dispatch(Command::Goto(start));
    control(&mut ui, KeyCode::Char('d'));
    ui.advance_search();
    assert!(ui.editor.is_searching_references());
    assert_eq!(
        ui.scan_position,
        Some(FileOffset::new((SEARCH_STEP_BYTES - 14) as u64))
    );
    assert!(screen(&mut ui, 80, 25).contains("Processing..."));
    key(&mut ui, KeyCode::Esc);
    assert!(!ui.editor.is_searching());
    assert_eq!(ui.editor.cursor(), start);
    assert_eq!(ui.editor.mode(), ViewMode::Hex);
    assert!(!ui.editor.document().is_dirty());
}

#[test]
fn search_text_is_utf8_and_hex_patterns_are_bounded() {
    let mut ui = ui("a\u{0436}z".as_bytes());
    key(&mut ui, KeyCode::F(7));
    key(&mut ui, KeyCode::Tab);
    type_text(&mut ui, "\u{0436}");
    key(&mut ui, KeyCode::Enter);
    ui.advance_search();
    assert_eq!(ui.editor.cursor(), FileOffset::new(1));
    assert_eq!(parse_hex_pattern("aB 01\tFF").unwrap(), [0xab, 1, 0xff]);
    for invalid in ["", "0", "xx", "0x12", "1 23", "\u{ff11}\u{ff12}"] {
        assert!(parse_hex_pattern(invalid).is_err(), "accepted {invalid}");
    }
    assert!(parse_hex_pattern(&"00".repeat(MAX_SEARCH_PATTERN_BYTES + 1)).is_err());
}

#[test]
fn key_release_and_modifier_collisions_do_not_trigger_default_bindings() {
    let mut ui = ui(b"ABC");
    let mut release = KeyEvent::new(KeyCode::F(3), KeyModifiers::NONE);
    release.kind = KeyEventKind::Release;
    ui.key(release);
    ui.key(KeyEvent::new(KeyCode::F(3), KeyModifiers::SHIFT));
    assert!(!ui.editor.is_editing());
    ui.key(KeyEvent::new(KeyCode::F(1), KeyModifiers::CONTROL));
    assert!(ui.dialog.is_none());
    ui.dispatch(Command::SetMode(ViewMode::Code));
    ui.dispatch(Command::SetDecodeMode(Some(
        hiewbie_core::instructions::DecodeMode::X86_64,
    )));
    ui.key(KeyEvent::new(
        KeyCode::F(1),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    ));
    assert_eq!(
        ui.editor.decode_context().mode,
        Some(hiewbie_core::instructions::DecodeMode::X86_64)
    );
}

#[test]
fn resize_empty_files_and_unicode_labels_render_without_panics() {
    let mut ui = ui(&[]);
    ui.label = escaped_text("\u{754c}\u{754c}e\u{301}\x1b[31m\n\u{202e}file");
    for (width, height) in [(0, 0), (1, 1), (22, 4), (23, 5), (80, 25), (132, 50)] {
        let output = screen(&mut ui, width, height);
        assert!(!output.contains('\x1b'));
        assert!(!output.contains('\u{202e}'));
    }
    let output = screen(&mut ui, 80, 25);
    assert!(output.contains("<empty file"));
    assert!(screen(&mut ui, 132, 50).contains("\\u{1b}[31m\\n\\u{202e}file"));
    key(&mut ui, KeyCode::F(3));
    type_text(&mut ui, "ff");
    assert!(!ui.editor.document().is_dirty());
}

struct LargeSource {
    len: u64,
    largest_read: Rc<Cell<usize>>,
}

impl ByteSource for LargeSource {
    fn len(&self) -> u64 {
        self.len
    }
    fn read_at(&mut self, offset: FileOffset, buffer: &mut [u8]) -> io::Result<usize> {
        self.largest_read
            .set(self.largest_read.get().max(buffer.len()));
        let count = self
            .len
            .saturating_sub(offset.get())
            .min(buffer.len() as u64) as usize;
        buffer[..count].fill(b'A');
        Ok(count)
    }
}

#[test]
fn large_offsets_and_large_windows_only_read_a_bounded_viewport() {
    let largest = Rc::new(Cell::new(0));
    let source = LargeSource {
        len: u64::MAX,
        largest_read: Rc::clone(&largest),
    };
    let mut ui = Ui::new(
        Editor::new(Document::new(source), false),
        "large sparse model".into(),
    );
    ui.dispatch(Command::Goto(FileOffset::new(0x1_0000_0010)));
    let output = screen(&mut ui, 80, 25);
    assert!(output.contains("00000001`00000010"));
    assert!(largest.get() <= 16 * 23);
    ui.dispatch(Command::SetMode(ViewMode::Text));
    screen(&mut ui, 300, 300);
    assert!(largest.get() <= hiewbie_core::MAX_READ_BYTES);
    ui.dispatch(Command::Goto(FileOffset::new(u64::MAX - 1)));
    let output = screen(&mut ui, 80, 25);
    assert!(output.contains("FFFFFFFF`FFFFFFFE"));
}

#[test]
fn text_view_wraps_binary_rows_and_scrolls_by_original_byte_offset() {
    let mut fixture = b"Alpha\tBeta\r\nBravo\nCharlie\rDelta\r\n".to_vec();
    fixture.extend([b'X'; 76]);
    fixture.extend(b"1234567890YZ\r\nnext\tend\xff");
    let mut ui = ui(&fixture);
    ui.dispatch(Command::SetMode(ViewMode::Text));

    let mut terminal = Terminal::new(TestBackend::new(80, 25)).unwrap();
    terminal.draw(|frame| ui.render(frame)).unwrap();
    let buffer = terminal.backend().buffer();
    let lines: Vec<_> = buffer
        .content
        .chunks(80)
        .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
        .collect();
    assert_eq!(lines[1].trim_end(), "↑Alpha○Beta");
    assert_eq!(lines[2].trim_end(), "▓Bravo◙Charlie♪Delta");
    assert_eq!(lines[3], format!("▓{}123", "X".repeat(76)));
    assert_eq!(lines[4].trim_end(), "▓4567890YZ");
    assert!(lines[5].starts_with("▓next○end\u{a0}"));
    assert!(lines[23].starts_with('▼'));
    assert_eq!(
        lines[24],
        " 1Help   2Unwrap 3       4Mode   5Goto   6LnFeed 7Search 8Table  9Files 10Quit  "
    );
    assert_eq!(buffer[(1, 1)].fg, ratatui::style::Color::LightCyan);
    assert_eq!(buffer[(79, 23)].bg, ratatui::style::Color::Blue);
    assert!(!ui.has_input_caret());

    key(&mut ui, KeyCode::Right);
    assert_eq!(ui.editor.cursor(), FileOffset::new(0));
    for offset in [12, 33, 112] {
        key(&mut ui, KeyCode::Down);
        assert_eq!(ui.editor.cursor(), FileOffset::new(offset));
    }
    key(&mut ui, KeyCode::Up);
    assert_eq!(ui.editor.cursor(), FileOffset::new(33));
    key(&mut ui, KeyCode::F(2));
    key(&mut ui, KeyCode::Right);
    assert_eq!(ui.editor.cursor(), FileOffset::new(33));
    let output = screen(&mut ui, 80, 25);
    assert!(output.lines().next().unwrap().contains("1     00000021│"));
    assert_eq!(
        output.lines().nth(1).unwrap(),
        format!("↑{}1234", "X".repeat(75))
    );
    assert_eq!(output.lines().nth(2).unwrap(), "▓ext○end");
    key(&mut ui, KeyCode::Enter);
    assert_eq!(ui.editor.mode(), ViewMode::Hex);
    assert_eq!(ui.editor.cursor(), FileOffset::new(33));
    assert_eq!(bytes(&mut ui), fixture);
    assert!(!ui.editor.document().is_dirty());
}

#[test]
fn text_line_feed_palette_selection_and_tabs_preserve_the_file_anchor() {
    use ratatui::style::Color;

    let fixture = b"run\tX\r\nsecond\tYZ\r\nlast\tW";
    let mut ui = ui(fixture);
    ui.dispatch(Command::SetMode(ViewMode::Text));
    assert_eq!(
        screen(&mut ui, 80, 25).lines().nth(1).unwrap(),
        "↑run     X"
    );
    key(&mut ui, KeyCode::F(6));
    let mut terminal = Terminal::new(TestBackend::new(80, 25)).unwrap();
    terminal.draw(|frame| ui.render(frame)).unwrap();
    let buffer = terminal.backend().buffer();
    let row = |y| {
        (26..53)
            .map(|x| buffer[(x, y)].symbol())
            .collect::<String>()
    };
    assert_eq!(row(9), "                        [x]");
    assert_eq!(row(10), "  ╔═ Select \"LineFeed\" ═╗  ");
    assert_eq!(buffer[(30, 11)].bg, Color::Black);
    assert_eq!(buffer[(30, 11)].fg, Color::White);
    assert_eq!(buffer[(37, 12)].fg, Color::Blue);
    assert_eq!(buffer[(54, 16)].bg, Color::Black);
    key(&mut ui, KeyCode::Down);
    key(&mut ui, KeyCode::Esc);
    assert_eq!(
        screen(&mut ui, 80, 25).lines().nth(2).unwrap(),
        "▓second  YZ"
    );

    key(&mut ui, KeyCode::F(6));
    key(&mut ui, KeyCode::F(3));
    assert_eq!(
        screen(&mut ui, 80, 25).lines().nth(1).unwrap(),
        "↑run     X♪"
    );
    key(&mut ui, KeyCode::Down);
    assert_eq!(ui.editor.cursor(), FileOffset::new(7));
    control(&mut ui, KeyCode::F(6));
    assert_eq!(
        screen(&mut ui, 80, 25).lines().nth(1).unwrap(),
        "↑second○YZ♪"
    );
    key(&mut ui, KeyCode::F(3));
    assert!(!ui.editor.is_editing());
    assert_eq!(bytes(&mut ui), fixture);
}

#[test]
fn text_null_line_feed_uses_zero_bytes_and_displays_crlf_as_glyphs() {
    let fixture = b"A\0B\0C\r\nD\0E";
    let mut ui = ui(fixture);
    ui.dispatch(Command::SetMode(ViewMode::Text));
    assert_eq!(screen(&mut ui, 80, 25).lines().nth(1).unwrap(), "↑A B C");
    key(&mut ui, KeyCode::F(6));
    key(&mut ui, KeyCode::F(4));
    let output = screen(&mut ui, 80, 25);
    assert_eq!(output.lines().nth(1).unwrap(), "↑A");
    assert_eq!(output.lines().nth(2).unwrap(), "▓B");
    assert_eq!(output.lines().nth(3).unwrap(), "▓C♪◙D");
    key(&mut ui, KeyCode::Down);
    assert_eq!(ui.editor.cursor(), FileOffset::new(2));
    assert_eq!(bytes(&mut ui), fixture);
}

#[test]
fn text_page_navigation_respects_display_height_and_the_unwrapped_line_limit() {
    let line = b"0123456789\n";
    let fixture = line.repeat(55);
    let mut ui = ui(&fixture);
    ui.dispatch(Command::SetMode(ViewMode::Text));
    screen(&mut ui, 80, 25);
    key(&mut ui, KeyCode::PageDown);
    assert_eq!(
        ui.editor.cursor(),
        FileOffset::new((23 * line.len()) as u64)
    );
    key(&mut ui, KeyCode::PageUp);
    assert_eq!(ui.editor.cursor(), FileOffset::new(0));

    let mut long = self::ui(&[b'A'; 2048]);
    long.dispatch(Command::SetMode(ViewMode::Text));
    screen(&mut long, 80, 25);
    key(&mut long, KeyCode::Down);
    assert_eq!(long.editor.cursor(), FileOffset::new(512));
    key(&mut long, KeyCode::Up);
    assert_eq!(long.editor.cursor(), FileOffset::new(0));
    key(&mut long, KeyCode::F(2));
    screen(&mut long, 40, 10);
    key(&mut long, KeyCode::PageDown);
    assert_eq!(long.editor.cursor(), FileOffset::new(39 * 8));
    key(&mut long, KeyCode::PageUp);
    assert_eq!(long.editor.cursor(), FileOffset::new(0));
}

/// Independently generated PE32+ headers and bytes; no executable is distributed.
fn pe_fixture() -> Vec<u8> {
    let mut bytes = vec![0; 0x400];
    bytes[..2].copy_from_slice(b"MZ");
    bytes[0x3c..0x40].copy_from_slice(&0x80_u32.to_le_bytes());
    bytes[0x80..0x84].copy_from_slice(b"PE\0\0");
    bytes[0x84..0x86].copy_from_slice(&0x8664_u16.to_le_bytes());
    bytes[0x86..0x88].copy_from_slice(&1_u16.to_le_bytes());
    bytes[0x94..0x96].copy_from_slice(&0xf0_u16.to_le_bytes());
    let optional = 0x98;
    bytes[optional..optional + 2].copy_from_slice(&0x20b_u16.to_le_bytes());
    bytes[optional + 16..optional + 20].copy_from_slice(&0x1000_u32.to_le_bytes());
    bytes[optional + 24..optional + 32].copy_from_slice(&0x1_4000_0000_u64.to_le_bytes());
    bytes[optional + 32..optional + 36].copy_from_slice(&0x1000_u32.to_le_bytes());
    bytes[optional + 36..optional + 40].copy_from_slice(&0x200_u32.to_le_bytes());
    bytes[optional + 56..optional + 60].copy_from_slice(&0x2000_u32.to_le_bytes());
    bytes[optional + 60..optional + 64].copy_from_slice(&0x200_u32.to_le_bytes());
    let section = optional + 0xf0;
    bytes[section..section + 5].copy_from_slice(b".text");
    bytes[section + 8..section + 12].copy_from_slice(&0x20_u32.to_le_bytes());
    bytes[section + 12..section + 16].copy_from_slice(&0x1000_u32.to_le_bytes());
    bytes[section + 16..section + 20].copy_from_slice(&0x200_u32.to_le_bytes());
    bytes[section + 20..section + 24].copy_from_slice(&0x200_u32.to_le_bytes());
    bytes[section + 36..section + 40].copy_from_slice(&0x6000_0020_u32.to_le_bytes());
    bytes[0x200..0x208].copy_from_slice(&[0xeb, 0x04, 0x90, 0x90, 0x90, 0x90, 0xc3, 0x90]);
    bytes
}

#[test]
fn hex_pe_uses_the_full_title_address_and_raw_offsets_during_edit() {
    let mut ui = ui(&pe_fixture());
    let compact = screen(&mut ui, 80, 25);
    assert!(
        compact
            .lines()
            .next()
            .unwrap()
            .contains("PE+.00000001`40000000│")
    );
    assert!(
        compact
            .lines()
            .nth(1)
            .unwrap()
            .starts_with("↑.40000000:  4D 5A")
    );
    let last_compact = screen(&mut ui, 88, 25);
    assert!(
        last_compact
            .lines()
            .nth(1)
            .unwrap()
            .starts_with("↑.40000000:")
    );
    assert!(last_compact.lines().last().unwrap().ends_with("11Hem"));
    let first_wide = screen(&mut ui, 89, 25);
    assert!(
        first_wide
            .lines()
            .nth(1)
            .unwrap()
            .starts_with("↑.00000001`40000000:")
    );
    assert!(first_wide.lines().last().unwrap().ends_with("11Hem   ▒"));

    let wide = screen(&mut ui, 100, 32);
    assert!(
        wide.lines()
            .nth(1)
            .unwrap()
            .starts_with("↑.00000001`40000000:  4D 5A")
    );
    assert_eq!(ui.bytes_per_row, 16);

    key(&mut ui, KeyCode::F(3));
    type_text(&mut ui, "12");
    let edit = screen(&mut ui, 100, 32);
    assert!(
        edit.lines()
            .next()
            .unwrap()
            .contains("FWO EDITMODE     PE+ 00000000`00000001│")
    );
    assert!(
        edit.lines()
            .nth(1)
            .unwrap()
            .starts_with("↑ 00000000`00000000:  12 5A")
    );
    key(&mut ui, KeyCode::Esc);
    assert_eq!(bytes(&mut ui), pe_fixture());
    assert!(
        screen(&mut ui, 80, 25)
            .lines()
            .nth(1)
            .unwrap()
            .starts_with("↑.40000000:  4D 5A")
    );
}

#[test]
fn hex_marks_only_the_selected_and_changed_cells_in_the_reference_columns() {
    use ratatui::style::Color;

    let mut ui = ui(&(0..=u8::MAX).cycle().take(4096).collect::<Vec<_>>());
    let mut terminal = Terminal::new(TestBackend::new(80, 25)).unwrap();
    terminal.draw(|frame| ui.render(frame)).unwrap();
    let frame = terminal.backend().buffer();
    for column in [13, 14, 62] {
        assert_eq!(frame[(column, 1)].fg, Color::Blue);
        assert_eq!(frame[(column, 1)].bg, Color::Gray);
    }
    assert_eq!(frame[(15, 1)].fg, Color::LightCyan);
    assert_eq!(frame[(9, 23)].symbol(), "0");
    assert_eq!(frame[(0, 23)].symbol(), "▼");
    assert_eq!(ui.visible_rows, 23);

    key(&mut ui, KeyCode::F(3));
    type_text(&mut ui, "A");
    terminal.draw(|frame| ui.render(frame)).unwrap();
    let frame = terminal.backend().buffer();
    assert_eq!(frame[(62, 1)].symbol(), "á");
    for column in [13, 14, 15, 62] {
        assert_eq!(frame[(column, 1)].fg, Color::LightYellow);
        assert_eq!(frame[(column, 1)].bg, Color::Blue);
    }
    assert!(!ui.editor.document().is_dirty());

    type_text(&mut ui, "F");
    terminal.draw(|frame| ui.render(frame)).unwrap();
    let frame = terminal.backend().buffer();
    assert_eq!(frame[(62, 1)].symbol(), "»");
    assert_eq!(frame[(62, 1)].fg, Color::LightYellow);
    assert_eq!(frame[(63, 1)].fg, Color::LightCyan);
    assert_eq!(bytes(&mut ui)[0], 0xAF);
}

fn code_at_entry() -> Ui<MemorySource> {
    let mut ui = ui(&pe_fixture());
    key(&mut ui, KeyCode::F(8));
    key(&mut ui, KeyCode::F(5));
    key(&mut ui, KeyCode::F(4));
    key(&mut ui, KeyCode::Down);
    key(&mut ui, KeyCode::Enter);
    assert_eq!(ui.editor.mode(), ViewMode::Code);
    ui
}

#[test]
fn decode_view_keeps_the_selected_byte_and_pages_over_compact_nops() {
    let mut ui = ui(&[0x90, 0xC3, 0xF8, 0xF9, 0xCC, 0x90, 0x90, 0xC3].repeat(8));
    key(&mut ui, KeyCode::Enter);
    let initial = screen(&mut ui, 80, 25);
    assert_eq!(
        initial.lines().nth(1),
        Some("↑ 00000000: 90                             nop"),
    );
    assert!(initial.lines().nth(6).unwrap().contains("00000005: 9090"));
    assert!(initial.lines().last().unwrap().contains("6Refer"));
    key(&mut ui, KeyCode::Down);
    assert_eq!(ui.editor.cursor().get(), 1);
    assert_eq!(
        screen(&mut ui, 80, 25).lines().nth(1),
        initial.lines().nth(1)
    );
    key(&mut ui, KeyCode::PageDown);
    assert_eq!(ui.editor.cursor().get(), 0x1a);
    assert!(
        screen(&mut ui, 80, 25)
            .lines()
            .nth(1)
            .unwrap()
            .contains("0000001A: F8")
    );
    key(&mut ui, KeyCode::PageUp);
    assert_eq!(ui.editor.cursor().get(), 0x19);
    assert_eq!(
        screen(&mut ui, 80, 25).lines().nth(1),
        initial.lines().nth(1)
    );
}

#[test]
fn raw_code_opens_in_x86_and_explicit_profiles_can_override_it() {
    let mut ui = ui(&[0x90, 0xc3]);
    ui.dispatch(Command::SetMode(ViewMode::Code));
    assert!(
        screen(&mut ui, 80, 25)
            .lines()
            .next()
            .unwrap()
            .contains("a32")
    );
    let output = screen(&mut ui, 80, 25);
    assert!(
        output
            .lines()
            .nth(1)
            .unwrap()
            .contains("90                             nop")
    );
    control(&mut ui, KeyCode::Char('a'));
    assert!(screen(&mut ui, 80, 25).contains("Decoder and raw addressing"));
    key(&mut ui, KeyCode::Char('3'));
    key(&mut ui, KeyCode::Enter);
    let output = screen(&mut ui, 80, 25);
    assert!(output.lines().next().unwrap().contains("64     00000000"));
    assert!(output.contains(" 00000000: 90"));
    assert!(output.contains("nop"));
    assert!(output.contains("ret"));
    control(&mut ui, KeyCode::Char('b'));
    assert!(
        screen(&mut ui, 80, 25)
            .lines()
            .next()
            .unwrap()
            .contains("16     00000000")
    );
    ui.key(KeyEvent::new(KeyCode::F(1), KeyModifiers::CONTROL));
    assert!(
        screen(&mut ui, 80, 25)
            .lines()
            .next()
            .unwrap()
            .contains("32     00000000")
    );
    ui.key(KeyEvent::new(KeyCode::F(1), KeyModifiers::SHIFT));
    assert!(matches!(ui.dialog, Some(Dialog::Analysis(_))));
}

#[test]
fn headers_navigate_entry_regions_and_exact_field_bytes() {
    let mut ui = ui(&pe_fixture());
    control(&mut ui, KeyCode::Char('o'));
    let output = screen(&mut ui, 80, 25);
    assert!(output.contains("Format PE32+ | machine x86-64 | 64 bit"));
    assert!(output.contains("Entry VA 0000000140001000"));
    key(&mut ui, KeyCode::F(5));
    assert!(ui.dialog.is_none());
    assert_eq!(ui.editor.cursor().get(), 0x200);
    key(&mut ui, KeyCode::F(8));
    key(&mut ui, KeyCode::F(6));
    assert!(screen(&mut ui, 80, 25).contains(".text FO 200+200 VA 140001000+20"));
    key(&mut ui, KeyCode::Down);
    key(&mut ui, KeyCode::Enter);
    assert_eq!(ui.editor.cursor().get(), 0x200);
    key(&mut ui, KeyCode::F(8));
    key(&mut ui, KeyCode::F(4));
    assert!(screen(&mut ui, 80, 25).contains("DOS.e_lfanew = 80  FO 3C+4"));
    key(&mut ui, KeyCode::Enter);
    assert_eq!(ui.editor.cursor().get(), 0x3c);
    assert_eq!(bytes(&mut ui), pe_fixture());
}

#[test]
fn virtual_goto_refuses_unmapped_addresses_without_moving() {
    let mut ui = ui(&pe_fixture());
    key(&mut ui, KeyCode::F(5));
    type_text(&mut ui, ".140001006");
    key(&mut ui, KeyCode::Enter);
    assert_eq!(ui.editor.cursor().get(), 0x206);
    key(&mut ui, KeyCode::F(5));
    type_text(&mut ui, ".140002000");
    key(&mut ui, KeyCode::Enter);
    assert_eq!(ui.editor.cursor().get(), 0x206);
    assert!(matches!(
        ui.dialog,
        Some(Dialog::Input(InputDialog { error: Some(_), .. }))
    ));
    assert!(screen(&mut ui, 80, 25).contains("no file-backed mapping"));
    key(&mut ui, KeyCode::Esc);
    assert_eq!(bytes(&mut ui), pe_fixture());
}

#[test]
fn code_branch_follow_return_and_instruction_steps_use_shared_commands() {
    let mut ui = code_at_entry();
    let output = screen(&mut ui, 100, 25);
    assert!(output.contains(".00000001`40001000: EB04"));
    assert!(output.contains("jmps"));
    assert!(output.lines().next().unwrap().contains("a64 PE+"));
    assert!(output.contains("--↓1"));
    key(&mut ui, KeyCode::Char('1'));
    assert_eq!(ui.editor.cursor().get(), 0x206);
    key(&mut ui, KeyCode::Backspace);
    assert_eq!(ui.editor.cursor().get(), 0x200);
    key(&mut ui, KeyCode::Down);
    assert_eq!(ui.editor.cursor().get(), 0x202);
    key(&mut ui, KeyCode::Up);
    assert_eq!(ui.editor.cursor().get(), 0x200);
    key(&mut ui, KeyCode::Right);
    assert_eq!(ui.editor.cursor().get(), 0x201);
    key(&mut ui, KeyCode::Char('/'));
    assert_eq!(ui.editor.cursor().get(), 0x201);
    assert_eq!(bytes(&mut ui), pe_fixture());
}

#[test]
fn code_operands_match_observed_x86_extension_rows() {
    for (mode, bytes, expected) in [
        (
            DecodeMode::X86_16,
            &[0x64, 0x8b, 0x40, 2][..],
            "mov          ax,fs:[bx][si][2]",
        ),
        (
            DecodeMode::X86_16,
            &[0x64, 0xa1, 0x34, 0x12],
            "mov          ax,fs:[01234]",
        ),
        (DecodeMode::X86_16, &[0xf3, 0xa4], "rep movsb"),
        (
            DecodeMode::X86_16,
            &[0x0f, 0x44, 0xc3],
            "cmovz        ax,bx",
        ),
        (
            DecodeMode::X86_32,
            &[0x8b, 0x44, 0x8b, 0xe0],
            "mov          eax,[ebx][ecx]*4[-020]",
        ),
        (
            DecodeMode::X86_32,
            &[0x64, 0xa1, 0x34, 0x12, 0, 0],
            "mov          eax,fs:[000001234]",
        ),
        (
            DecodeMode::X86_32,
            &[0xdb, 0x6c, 0x24, 0x10],
            "fld          t,[esp][010]",
        ),
        (
            DecodeMode::X86_64,
            &[0xdd, 0x44, 0x24, 0x10],
            "fld          q,[rsp][010]",
        ),
        (
            DecodeMode::X86_64,
            &[0x4c, 0x8b, 0x0d, 0x20, 0, 0, 0],
            "mov          r9,[000000027]",
        ),
        (
            DecodeMode::X86_64,
            &[0x66, 0x0f, 0x3a, 0x63, 0xca, 0],
            "pcmpistri    xmm1,xmm2,0",
        ),
        (
            DecodeMode::X86_64,
            &[0x66, 0x0f, 0x3a, 0x44, 0xca, 1],
            "pclmulqdq    xmm1,xmm2,1",
        ),
        (
            DecodeMode::X86_64,
            &[0xc4, 0xe2, 0x6d, 0x98, 0xcb],
            "vfmadd132ps  ymm1,ymm2,ymm3",
        ),
        (DecodeMode::X86_64, &[0x60], "#UD(64)"),
    ] {
        let mut ui = ui(bytes);
        ui.editor
            .dispatch(Command::SetDecodeMode(Some(mode)))
            .unwrap();
        ui.editor
            .dispatch(Command::SetMode(ViewMode::Code))
            .unwrap();
        let output = screen(&mut ui, 120, 25);
        let row = output.lines().nth(1).unwrap();
        let instruction: String = row.chars().skip(43).collect();
        assert_eq!(instruction, expected, "{mode:?} {bytes:02X?}");
    }
}

#[test]
fn code_matches_the_observed_16_bit_instruction_rows() {
    for row in include_str!("../../../../tests/fixtures/x86-16-hiew-8.63.tsv").lines() {
        if row.starts_with('#') {
            continue;
        }
        let fields: Vec<_> = row.split('\t').collect();
        let [_, _, _, hex, _, _, opcodes, expected] = fields.as_slice() else {
            panic!("malformed fixture row: {row}");
        };
        let bytes: Vec<_> = (0..hex.len())
            .step_by(2)
            .map(|index| u8::from_str_radix(&hex[index..index + 2], 16).unwrap())
            .collect();
        let mut ui = ui(&bytes);
        ui.editor
            .dispatch(Command::SetDecodeMode(Some(DecodeMode::X86_16)))
            .unwrap();
        key(&mut ui, KeyCode::Enter);
        let output = screen(&mut ui, 120, 25);
        let instruction = output.lines().nth(1).unwrap();
        let opcode_cells: String = instruction.chars().skip(12).take(30).collect();
        let operand_cells: String = instruction.chars().skip(43).collect();
        assert_eq!(opcode_cells.trim_end(), *opcodes, "{row}");
        assert_eq!(operand_cells.trim_end(), *expected, "{row}");
    }
}

#[test]
fn code_uses_encoded_x86_displacements_and_hides_implicit_fld_destination() {
    for (mode, bytes, expected) in [
        (DecodeMode::X86_16, &[0xd9, 0xc0][..], "fld          st(0)"),
        (
            DecodeMode::X86_16,
            &[0x8b, 0x40, 0x0a],
            "mov          ax,[bx][si][00A]",
        ),
        (
            DecodeMode::X86_16,
            &[0x8b, 0x80, 0x0a, 0],
            "mov          ax,[bx][si][0000A]",
        ),
        (
            DecodeMode::X86_16,
            &[0xc5, 0xb8, 0x89, 0xff],
            "lds          di,[bx][si][-00077]",
        ),
        (
            DecodeMode::X86_16,
            &[0x8b, 0x80, 0, 0],
            "mov          ax,[bx][si][0]",
        ),
        (DecodeMode::X86_16, &[0x8b, 6, 4, 0], "mov          ax,[4]"),
        (
            DecodeMode::X86_16,
            &[0x67, 0x8b, 5, 0x34, 0x12, 0, 0],
            "mov          ax,[000001234]",
        ),
        (
            DecodeMode::X86_16,
            &[0x67, 0x8b, 0x80, 0x0a, 0, 0, 0],
            "mov          ax,[eax][00000000A]",
        ),
        (
            DecodeMode::X86_32,
            &[0x67, 0x8b, 6, 0x34, 0x12],
            "mov          eax,[01234]",
        ),
    ] {
        let mut ui = ui(bytes);
        ui.editor
            .dispatch(Command::SetDecodeMode(Some(mode)))
            .unwrap();
        key(&mut ui, KeyCode::Enter);
        let output = screen(&mut ui, 120, 25);
        let displayed: String = output.lines().nth(1).unwrap().chars().skip(43).collect();
        assert_eq!(displayed.trim_end(), expected, "{bytes:02X?}");
    }
}

#[test]
fn code_matches_the_observed_32_and_64_bit_extension_rows() {
    for row in include_str!("../../../../tests/fixtures/x86-32-64-hiew-8.63.tsv").lines() {
        if row.starts_with('#') {
            continue;
        }
        let fields: Vec<_> = row.split('\t').collect();
        let [bits, _, offset, _, hex, _, _, opcodes, expected] = fields.as_slice() else {
            panic!("malformed fixture row: {row}");
        };
        let mode = match *bits {
            "32" => DecodeMode::X86_32,
            "64" => DecodeMode::X86_64,
            _ => panic!("unexpected fixture bitness: {bits}"),
        };
        let offset = usize::from_str_radix(offset, 16).unwrap();
        let mut bytes = vec![0x90; offset];
        bytes.extend(
            (0..hex.len())
                .step_by(2)
                .map(|index| u8::from_str_radix(&hex[index..index + 2], 16).unwrap()),
        );
        let mut ui = ui(&bytes);
        ui.editor
            .dispatch(Command::SetDecodeMode(Some(mode)))
            .unwrap();
        ui.editor
            .dispatch(Command::Goto(FileOffset::new(offset as u64)))
            .unwrap();
        key(&mut ui, KeyCode::Enter);
        ui.top = offset as u64;
        let output = screen(&mut ui, 120, 25);
        let instruction = output.lines().nth(1).unwrap();
        let opcode_cells: String = instruction.chars().skip(12).take(30).collect();
        let operand_cells: String = instruction.chars().skip(43).collect();
        assert_eq!(opcode_cells.trim_end(), *opcodes, "{row}");
        assert_eq!(operand_cells.trim_end(), *expected, "{row}");
    }
}

#[test]
fn code_pe64_absolute_memory_uses_split_addresses_only_when_wide() {
    let mut image = pe_fixture();
    let instructions = [
        0x4c, 0x8b, 0x0d, 0x20, 0, 0, 0, 0x65, 0x8b, 0x04, 0x25, 0x34, 0x12, 0, 0,
    ];
    image[0x200..0x200 + instructions.len()].copy_from_slice(&instructions);
    let mut ui = ui(&image);
    ui.editor.dispatch(Command::GotoEntry).unwrap();
    ui.editor
        .dispatch(Command::SetMode(ViewMode::Code))
        .unwrap();
    let output = screen(&mut ui, 120, 25);
    assert!(output.contains("mov          r9,[00000001`40001027]"));
    assert!(output.contains("mov          eax,gs:[000001234]"));
}

#[test]
fn code_pe64_operand_labels_open_hex_at_the_byte_and_restore_the_code_window() {
    let mut image = pe_fixture();
    // Data at an unaligned byte followed by a near-jump slot at offset 0x218.
    image[0x200..0x20d].copy_from_slice(&[0x4c, 0x8b, 0x0d, 10, 0, 0, 0, 0xff, 0x25, 11, 0, 0, 0]);
    image[0x211] = 0x5a;
    image[0x217] = 0xc3;
    image[0x218..0x220].copy_from_slice(&0x1_4000_1017_u64.to_le_bytes());
    let mut ui = ui(&image);
    ui.editor.dispatch(Command::GotoEntry).unwrap();
    key(&mut ui, KeyCode::Enter);
    let code = screen(&mut ui, 120, 25);
    assert!(code.contains("mov          r9,[00000001`40001011] --↓1"));
    assert!(code.contains("jmp          q,[00000001`40001018] --↓2"));
    key(&mut ui, KeyCode::Down);
    let _ = screen(&mut ui, 120, 25);
    key(&mut ui, KeyCode::Char('1'));
    assert_eq!(ui.editor.mode(), ViewMode::Hex);
    assert_eq!(ui.editor.cursor().get(), 0x211);
    assert!(
        screen(&mut ui, 120, 25)
            .lines()
            .nth(1)
            .unwrap()
            .starts_with("↑.00000001`40001010:  00 5A")
    );
    key(&mut ui, KeyCode::Backspace);
    assert_eq!(ui.editor.mode(), ViewMode::Code);
    assert_eq!(ui.editor.cursor().get(), 0x207);
    assert_eq!(ui.top, 0x200);
    let _ = screen(&mut ui, 120, 25);
    key(&mut ui, KeyCode::Char('2'));
    assert_eq!(ui.editor.mode(), ViewMode::Code);
    assert_eq!(ui.editor.cursor().get(), 0x217);
    assert_eq!(bytes(&mut ui), image);
}

#[test]
fn code_edit_restore_keys_preserve_the_view_branch_return_history() {
    let original = pe_fixture();
    let mut ui = code_at_entry();
    screen(&mut ui, 80, 25);
    key(&mut ui, KeyCode::Char('1'));
    assert_eq!(ui.editor.cursor().get(), 0x206);

    key(&mut ui, KeyCode::F(3));
    type_text(&mut ui, "CC");
    assert_eq!(ui.editor.cursor().get(), 0x207);
    assert_eq!(bytes(&mut ui)[0x206], 0xcc);
    key(&mut ui, KeyCode::Backspace);
    assert!(ui.editor.is_editing());
    assert_eq!(ui.editor.cursor().get(), 0x206);
    assert_eq!(bytes(&mut ui), original);
    assert!(!ui.editor.document().is_dirty());

    type_text(&mut ui, "CC");
    key(&mut ui, KeyCode::Left);
    key(&mut ui, KeyCode::F(3));
    assert_eq!(ui.editor.cursor().get(), 0x206);
    assert_eq!(bytes(&mut ui), original);
    assert!(screen(&mut ui, 80, 25).contains("EDITMODE"));
    assert!(ui.editor.is_editing());

    key(&mut ui, KeyCode::Esc);
    key(&mut ui, KeyCode::Backspace);
    assert!(!ui.editor.is_editing());
    assert_eq!(ui.editor.cursor().get(), 0x200);
    assert_eq!(bytes(&mut ui), original);
}

#[test]
fn code_edit_refreshes_branch_bytes_and_undo_without_assembly() {
    let mut ui = code_at_entry();
    key(&mut ui, KeyCode::F(3));
    key(&mut ui, KeyCode::Right);
    type_text(&mut ui, "02");
    ui.dispatch(Command::Goto(FileOffset::new(0x200)));
    let output = screen(&mut ui, 80, 25);
    assert!(output.contains("EDITMODE"));
    assert!(ui.editor.document().is_dirty());
    assert!(output.contains("EB02"));
    assert!(output.contains("2Asm"));
    assert_eq!(
        ui.editor.code_rows(1).unwrap()[0]
            .instruction
            .direct_branch
            .unwrap()
            .address
            .get(),
        0x1_4000_1004
    );
    control(&mut ui, KeyCode::Char('z'));
    assert!(screen(&mut ui, 80, 25).contains("EB04"));
    assert_eq!(
        ui.editor.code_rows(1).unwrap()[0]
            .instruction
            .direct_branch
            .unwrap()
            .address
            .get(),
        0x1_4000_1006
    );
    key(&mut ui, KeyCode::Esc);
    assert_eq!(bytes(&mut ui), pe_fixture());
    assert!(!ui.editor.document().is_dirty());
}

#[test]
fn code_edit_keeps_raw_bytes_visible_when_header_changes_break_detection() {
    let mut ui = code_at_entry();
    ui.dispatch(Command::Goto(FileOffset::new(0x80)));
    key(&mut ui, KeyCode::F(3));
    type_text(&mut ui, "00");
    ui.dispatch(Command::Goto(FileOffset::new(0x80)));
    let output = screen(&mut ui, 80, 25);
    assert!(output.contains("EDITMODE"));
    assert!(ui.editor.document().is_dirty());
    assert!(output.contains("Code unavailable; raw bytes:"));
    assert!(output.contains("0000000000000080  00"));
    assert!(ui.editor.is_editing());
    control(&mut ui, KeyCode::Char('z'));
    let output = screen(&mut ui, 80, 25);
    assert!(output.lines().next().unwrap().contains("a64 PE+"));
    assert!(!output.contains("Code unavailable; raw bytes:"));
    key(&mut ui, KeyCode::Esc);
    assert_eq!(bytes(&mut ui), pe_fixture());
}

#[test]
fn raw_base_endian_and_malformed_headers_remain_explicit() {
    let mut malformed = vec![0; 64];
    malformed[..2].copy_from_slice(b"MZ");
    let mut ui = ui(&malformed);
    key(&mut ui, KeyCode::F(8));
    let output = screen(&mut ui, 80, 25);
    assert!(output.contains("raw bytes; executable metadata is unavailable"));
    assert!(output.contains("PE header overlaps the DOS header"));
    key(&mut ui, KeyCode::Esc);
    control(&mut ui, KeyCode::Char('a'));
    key(&mut ui, KeyCode::Char('v'));
    type_text(&mut ui, "100000000");
    key(&mut ui, KeyCode::Enter);
    control(&mut ui, KeyCode::Char('a'));
    key(&mut ui, KeyCode::Char('6'));
    key(&mut ui, KeyCode::Char('b'));
    key(&mut ui, KeyCode::Enter);
    let output = screen(&mut ui, 80, 25);
    assert!(output.lines().next().unwrap().contains("A64"));
    assert!(output.contains("unsupported"));
    control(&mut ui, KeyCode::Char('a'));
    assert!(screen(&mut ui, 100, 25).contains("aarch64 / big / override / raw base 100000000"));
    key(&mut ui, KeyCode::Esc);
    key(&mut ui, KeyCode::F(5));
    type_text(&mut ui, ".100000004");
    key(&mut ui, KeyCode::Enter);
    assert_eq!(ui.editor.cursor().get(), 4);
    assert_eq!(bytes(&mut ui), malformed);
    for invalid in ["", "+1", "-1", "10000000000000000", "١"] {
        assert!(analysis::parse_base(invalid).is_err(), "accepted {invalid}");
    }
    assert_eq!(
        analysis::parse_base("18446744073709551615t").unwrap(),
        u64::MAX
    );
    assert_eq!(analysis::parse_base(".10").unwrap(), 0x10);
}

#[test]
fn universal_slice_picker_updates_the_actual_image_and_file_cursor() {
    let mut bytes = vec![0; 768];
    bytes[..4].copy_from_slice(&0xcafe_babe_u32.to_be_bytes());
    bytes[4..8].copy_from_slice(&2_u32.to_be_bytes());
    for (index, cpu) in [0x0100_0007_u32, 0x0100_000c].into_iter().enumerate() {
        let table = 8 + index * 20;
        let offset = (index + 1) * 256;
        bytes[table..table + 4].copy_from_slice(&cpu.to_be_bytes());
        bytes[table + 8..table + 12].copy_from_slice(&(offset as u32).to_be_bytes());
        bytes[table + 12..table + 16].copy_from_slice(&256_u32.to_be_bytes());
        bytes[table + 16..table + 20].copy_from_slice(&8_u32.to_be_bytes());
        let slice = &mut bytes[offset..offset + 256];
        slice[..4].copy_from_slice(&0xfeed_facf_u32.to_le_bytes());
        slice[4..8].copy_from_slice(&cpu.to_le_bytes());
        slice[12..16].copy_from_slice(&2_u32.to_le_bytes());
        slice[16..20].copy_from_slice(&1_u32.to_le_bytes());
        slice[20..24].copy_from_slice(&72_u32.to_le_bytes());
        slice[32..36].copy_from_slice(&0x19_u32.to_le_bytes());
        slice[36..40].copy_from_slice(&72_u32.to_le_bytes());
        slice[40..46].copy_from_slice(b"__TEXT");
        slice[56..64].copy_from_slice(&0x1_0000_0000_u64.to_le_bytes());
        slice[64..72].copy_from_slice(&256_u64.to_le_bytes());
        slice[80..88].copy_from_slice(&256_u64.to_le_bytes());
        slice[92..96].copy_from_slice(&5_u32.to_le_bytes());
    }
    let mut ui = ui(&bytes);
    key(&mut ui, KeyCode::F(8));
    key(&mut ui, KeyCode::Char('s'));
    assert!(screen(&mut ui, 80, 25).contains("Universal Mach-O slices"));
    key(&mut ui, KeyCode::Down);
    key(&mut ui, KeyCode::Enter);
    let output = screen(&mut ui, 80, 25);
    assert!(output.contains("Selected slice 1"));
    assert!(output.contains("machine AArch64"));
    assert!(output.contains("mapped image"));
    assert!(!output.contains("base 0"));
    assert_eq!(ui.editor.cursor().get(), 512);
    assert_eq!(ui.editor.executable().unwrap().selected_slice, Some(1));
}

#[test]
fn code_viewport_and_small_windows_keep_reads_and_allocations_bounded() {
    let largest = Rc::new(Cell::new(0));
    let source = LargeSource {
        len: u64::MAX,
        largest_read: Rc::clone(&largest),
    };
    let mut ui = Ui::new(
        Editor::new(Document::new(source), false),
        "large raw image".into(),
    );
    ui.dispatch(Command::SetDecodeMode(Some(
        hiewbie_core::instructions::DecodeMode::X86_64,
    )));
    ui.dispatch(Command::SetMode(ViewMode::Code));
    ui.dispatch(Command::Goto(FileOffset::new(0x1_0000_0010)));
    for (width, height) in [(0, 0), (1, 1), (22, 4), (23, 5), (80, 25), (300, 300)] {
        screen(&mut ui, width, height);
    }
    assert!(largest.get() <= 15);
    assert!(ui.visible_rows <= hiewbie_core::MAX_CODE_ROWS);
    assert!(screen(&mut ui, 80, 25).contains("00000001`00000010"));
}

#[test]
fn search_cells_show_both_representations_direction_and_context() {
    let mut ui = ui(b"ABC abc AbC");
    key(&mut ui, KeyCode::F(7));
    type_text(&mut ui, "41 62 43");
    key(&mut ui, KeyCode::Tab);
    key(&mut ui, KeyCode::F(2));
    let cells = screen(&mut ui, 80, 25);
    let panel = cells
        .lines()
        .skip(10)
        .take(5)
        .map(|line| line.chars().skip(4).take(71).collect::<String>())
        .collect::<Vec<_>>();
    let title = "══[Backward •File •noCase•Even]";
    let text_field = format!(" ASCII: AbC{}", "░".repeat(17));
    let hex_field = format!("   Hex: 41 62 43{}", "░".repeat(52));
    assert_eq!(
        panel,
        [
            format!("╔{title}{}╗", "═".repeat(69 - title.chars().count())),
            format!("║{text_field:<69}║"),
            format!("║{:69}║", ""),
            format!("║{hex_field:<69}║"),
            format!("╚{}╝", "═".repeat(69)),
        ]
    );
    assert_eq!(
        cells.lines().last(),
        Some(" 1Help   2Direct 3       4Area   5GoLast 6       7       8       9      10")
    );
    key(&mut ui, KeyCode::F(1));
    assert!(screen(&mut ui, 80, 25).contains("UTF-8 folds ASCII letters only"));
    key(&mut ui, KeyCode::Esc);
    assert_eq!(screen(&mut ui, 80, 25), cells);
    key(&mut ui, KeyCode::Esc);
    assert!(ui.dialog.is_none());
    assert_eq!(ui.editor.cursor(), FileOffset::new(0));
}

#[test]
fn search_keeps_exact_hex_separate_and_rejects_unconvertible_input() {
    let mut ui = ui(b"ABC abc AbC\xff");
    key(&mut ui, KeyCode::F(7));
    type_text(&mut ui, "FF");
    key(&mut ui, KeyCode::Tab);
    assert!(screen(&mut ui, 80, 25).contains("Hex query is not valid UTF-8"));
    key(&mut ui, KeyCode::Enter);
    ui.advance_search();
    assert_eq!(ui.editor.cursor(), FileOffset::new(11));

    control(&mut ui, KeyCode::Home);
    key(&mut ui, KeyCode::F(7));
    type_text(&mut ui, "61 62 63");
    key(&mut ui, KeyCode::Enter);
    ui.advance_search();
    assert_eq!(ui.editor.cursor(), FileOffset::new(4));
    control(&mut ui, KeyCode::Home);
    key(&mut ui, KeyCode::F(7));
    key(&mut ui, KeyCode::F(5));
    assert!(ui.dialog.is_none());
    assert_eq!(ui.editor.cursor(), FileOffset::new(4));
}

#[test]
fn backward_text_search_uses_last_hit_and_explicit_origin_resets() {
    let mut ui = ui(b"ABC\nabc\nAbC");
    key(&mut ui, KeyCode::F(4));
    key(&mut ui, KeyCode::F(1));
    control(&mut ui, KeyCode::End);
    key(&mut ui, KeyCode::F(7));
    type_text(&mut ui, "aBc");
    key(&mut ui, KeyCode::F(2));
    key(&mut ui, KeyCode::Enter);
    ui.advance_search();
    assert_eq!(ui.editor.cursor(), FileOffset::new(8));

    key(&mut ui, KeyCode::Up);
    assert_eq!(ui.editor.cursor(), FileOffset::new(4));
    key(&mut ui, KeyCode::Up);
    assert_eq!(ui.editor.cursor(), FileOffset::new(0));
    control(&mut ui, KeyCode::Char('r'));
    ui.advance_search();
    assert_eq!(ui.editor.cursor(), FileOffset::new(4));

    key(&mut ui, KeyCode::F(5));
    type_text(&mut ui, "9");
    key(&mut ui, KeyCode::Enter);
    control(&mut ui, KeyCode::Char('r'));
    ui.advance_search();
    assert_eq!(ui.editor.cursor(), FileOffset::new(8));

    control(&mut ui, KeyCode::Home);
    control(&mut ui, KeyCode::Char('r'));
    ui.advance_search();
    assert_eq!(ui.editor.cursor(), FileOffset::new(0));

    control(&mut ui, KeyCode::End);
    control(&mut ui, KeyCode::Char('r'));
    ui.advance_search();
    assert_eq!(ui.editor.cursor(), FileOffset::new(8));
    assert_eq!(bytes(&mut ui), b"ABC\nabc\nAbC");
    assert!(!ui.editor.document().is_dirty());
}

#[test]
fn block_marking_updates_hex_and_text_cells_and_escape_cancels_tracking() {
    use ratatui::style::Color;

    let mut ui = ui(b"abcdefghijklmnop");
    key(&mut ui, KeyCode::Right);
    key(&mut ui, KeyCode::Right);
    key(&mut ui, KeyCode::Right);
    key(&mut ui, KeyCode::Char('*'));
    key(&mut ui, KeyCode::Left);
    key(&mut ui, KeyCode::Left);
    let frame = screen(&mut ui, 80, 25);
    assert!(frame.lines().nth(23).unwrap().ends_with("00000003/3"));

    key(&mut ui, KeyCode::Char('*'));
    key(&mut ui, KeyCode::Left);
    let mut terminal = Terminal::new(TestBackend::new(80, 25)).unwrap();
    terminal.draw(|frame| ui.render(frame)).unwrap();
    let buffer = terminal.backend().buffer();
    for column in [16, 17, 19, 20, 22, 23, 63, 64, 65] {
        let cell = &buffer[(column, 1)];
        assert_eq!(cell.fg, Color::Yellow, "column {column}");
        assert_eq!(cell.bg, Color::Magenta, "column {column}");
    }
    let frame = screen(&mut ui, 80, 25);
    assert!(!frame.lines().nth(23).unwrap().contains("00000003/3"));
    assert!(frame.lines().last().unwrap().contains("2PutBlk"));

    key(&mut ui, KeyCode::Char(']'));
    assert_eq!(ui.editor.cursor(), FileOffset::new(3));
    key(&mut ui, KeyCode::Char('['));
    assert_eq!(ui.editor.cursor(), FileOffset::new(1));
    key(&mut ui, KeyCode::Char('*'));
    key(&mut ui, KeyCode::Char('*'));
    assert_eq!(key(&mut ui, KeyCode::Esc), UiAction::Continue);
    assert!(ui.editor.block_selection().is_none());
    assert!(screen(&mut ui, 80, 25).contains("Block marking canceled"));
    assert_eq!(bytes(&mut ui), b"abcdefghijklmnop");
    assert!(!ui.editor.document().is_dirty());
}

#[test]
fn scoped_search_keeps_the_completed_block_and_its_original_bounds_for_repeat() {
    let mut ui = ui(b"A--A--A--A");
    key(&mut ui, KeyCode::F(7));
    key(&mut ui, KeyCode::F(4));
    assert!(screen(&mut ui, 80, 25).contains("Finish marking a block"));
    key(&mut ui, KeyCode::Esc);
    for _ in 0..3 {
        key(&mut ui, KeyCode::Right);
    }
    key(&mut ui, KeyCode::Char('*'));
    for _ in 0..3 {
        key(&mut ui, KeyCode::Right);
    }
    key(&mut ui, KeyCode::Char('*'));
    key(&mut ui, KeyCode::Char('['));
    key(&mut ui, KeyCode::F(7));
    key(&mut ui, KeyCode::F(4));
    let frame = screen(&mut ui, 80, 25);
    assert!(frame.contains("[Forward •Block •Case  •Even]"));
    assert!(frame.lines().last().unwrap().contains("4Area"));
    type_text(&mut ui, "41");
    key(&mut ui, KeyCode::Enter);
    ui.advance_search();
    assert_eq!(ui.editor.cursor(), FileOffset::new(3));
    control(&mut ui, KeyCode::Char('r'));
    ui.advance_search();
    assert_eq!(ui.editor.cursor(), FileOffset::new(6));
    key(&mut ui, KeyCode::Char('*'));
    assert!(ui.editor.block_selection().is_none());
    control(&mut ui, KeyCode::Char('r'));
    ui.advance_search();
    assert_eq!(ui.editor.cursor(), FileOffset::new(6));
    assert!(ui.message.contains("No match"));
    assert_eq!(bytes(&mut ui), b"A--A--A--A");
    assert!(!ui.editor.document().is_dirty());
}

#[test]
fn block_export_form_preserves_range_and_requires_explicit_existing_file_confirmation() {
    use hiewbie::file::ExportOffset;
    use std::path::PathBuf;

    let mut ui = ui(b"abcdef");
    control(&mut ui, KeyCode::Char('k'));
    assert_eq!(key(&mut ui, KeyCode::F(2)), UiAction::Continue);
    let frame = screen(&mut ui, 80, 25);
    assert!(frame.contains("Write block"));
    assert!(frame.contains("Block  00000000–00000005"));
    assert!(frame.contains("Length 00000006/6"));
    assert!(frame.contains("Format  Raw bytes"));
    type_text(&mut ui, "část.bin");
    assert_eq!(key(&mut ui, KeyCode::Enter), UiAction::Continue);
    type_text(&mut ui, "*");
    assert_eq!(key(&mut ui, KeyCode::Enter), UiAction::Continue);
    let UiAction::ExportBlock(first) = key(&mut ui, KeyCode::Enter) else {
        panic!("the form must emit an export request");
    };
    assert_eq!(first.offset, ExportOffset::Append);
    assert_eq!(first.path, PathBuf::from("část.bin"));
    assert_eq!(first.range.len, 6);
    assert!(!first.replace_existing);

    ui.confirm_block_output();
    assert!(screen(&mut ui, 80, 25).contains("Enter confirms the update"));
    let UiAction::ExportBlock(confirmed) = key(&mut ui, KeyCode::Enter) else {
        panic!("the confirmation must emit an export request");
    };
    assert!(confirmed.replace_existing);
    ui.export_in_progress = true;
    assert_eq!(key(&mut ui, KeyCode::Esc), UiAction::CancelBlockExport);
    ui.report_export_failure("Block export canceled");
    assert!(screen(&mut ui, 80, 25).contains("Block export canceled"));
    assert_eq!(key(&mut ui, KeyCode::Esc), UiAction::Continue);
    assert!(ui.dialog.is_none());
    assert_eq!(bytes(&mut ui), b"abcdef");
    assert!(!ui.editor.document().is_dirty());
}

#[test]
fn code_block_highlight_uses_individual_instruction_bytes() {
    use hiewbie_core::instructions::DecodeMode;
    use ratatui::style::Color;

    let mut ui = ui(&[0xE9, 0, 0, 0, 0, 0xCC]);
    key(&mut ui, KeyCode::Right);
    key(&mut ui, KeyCode::Char('*'));
    key(&mut ui, KeyCode::Right);
    key(&mut ui, KeyCode::Char('*'));
    ui.editor
        .dispatch(Command::SetDecodeMode(Some(DecodeMode::X86_32)))
        .unwrap();
    ui.editor
        .dispatch(Command::SetMode(ViewMode::Code))
        .unwrap();
    ui.editor
        .dispatch(Command::Goto(FileOffset::new(0)))
        .unwrap();
    let mut terminal = Terminal::new(TestBackend::new(80, 25)).unwrap();
    terminal.draw(|frame| ui.render(frame)).unwrap();
    let cells = terminal.backend().buffer();
    assert_eq!(cells[(12, 1)].symbol(), "E");
    for column in 12..22 {
        let bg = cells[(column, 1)].bg;
        assert_eq!(
            bg == Color::Magenta,
            (14..18).contains(&column),
            "column {column}"
        );
    }
}

#[test]
fn native_caret_follows_hex_nibbles_ascii_and_focused_search_field() {
    let mut ui = ui(&[0; 32]);
    let mut terminal = Terminal::new(TestBackend::new(80, 25)).unwrap();
    assert!(!ui.has_input_caret());
    key(&mut ui, KeyCode::F(3));
    terminal.draw(|frame| ui.render(frame)).unwrap();
    assert!(ui.has_input_caret());
    terminal.backend_mut().assert_cursor_position((13, 1));

    key(&mut ui, KeyCode::Char('A'));
    terminal.draw(|frame| ui.render(frame)).unwrap();
    terminal.backend_mut().assert_cursor_position((14, 1));
    assert_eq!(ui.pending_nibble, Some(0xA));
    assert!(!ui.editor.document().is_dirty());

    key(&mut ui, KeyCode::Char('F'));
    terminal.draw(|frame| ui.render(frame)).unwrap();
    terminal.backend_mut().assert_cursor_position((16, 1));
    assert_eq!(bytes(&mut ui)[0], 0xAF);
    key(&mut ui, KeyCode::Tab);
    terminal.draw(|frame| ui.render(frame)).unwrap();
    assert!(ui.has_input_caret());
    terminal.backend_mut().assert_cursor_position((63, 1));

    key(&mut ui, KeyCode::Esc);
    assert!(!ui.has_input_caret());
    assert_eq!(bytes(&mut ui), [0; 32]);
    key(&mut ui, KeyCode::F(7));
    terminal.draw(|frame| ui.render(frame)).unwrap();
    assert!(ui.has_input_caret());
    terminal.backend_mut().assert_cursor_position((13, 13));
    key(&mut ui, KeyCode::Tab);
    type_text(&mut ui, "界");
    terminal.draw(|frame| ui.render(frame)).unwrap();
    terminal.backend_mut().assert_cursor_position((15, 11));
    key(&mut ui, KeyCode::Esc);
    assert!(!ui.has_input_caret());
    assert_eq!(ui.editor.cursor(), FileOffset::new(1));
    key(&mut ui, KeyCode::F(5));
    terminal.draw(|frame| ui.render(frame)).unwrap();
    assert!(ui.has_input_caret());
    terminal.backend_mut().assert_cursor_position((1, 1));
    key(&mut ui, KeyCode::Esc);
    assert!(!ui.has_input_caret());
}

#[test]
fn picker_keys_and_help_select_without_moving_the_file_cursor() {
    use ratatui::style::Color;

    let mut ui = ui(b"abcdefgh");
    key(&mut ui, KeyCode::Right);
    key(&mut ui, KeyCode::F(4));
    let mut terminal = Terminal::new(TestBackend::new(80, 25)).unwrap();
    terminal.draw(|frame| ui.render(frame)).unwrap();
    let cells = terminal.backend().buffer();
    assert_eq!(cells[(28, 12)].bg, Color::Cyan);
    assert_eq!(cells[(29, 12)].bg, Color::Black);
    assert_eq!(cells[(49, 12)].bg, Color::Black);
    assert_eq!(cells[(50, 12)].bg, Color::Cyan);
    assert_eq!(cells[(54, 10)].bg, Color::Black);
    assert_eq!(cells[(51, 9)].symbol(), "[");

    key(&mut ui, KeyCode::Up);
    key(&mut ui, KeyCode::F(4));
    assert_eq!(ui.editor.mode(), ViewMode::Code);
    assert!(ui.dialog.is_none());
    key(&mut ui, KeyCode::F(4));
    for (choice, mode) in [
        (1, ViewMode::Text),
        (2, ViewMode::Hex),
        (3, ViewMode::Code),
        (4, ViewMode::Text),
    ] {
        key(&mut ui, KeyCode::F(choice));
        assert_eq!(ui.editor.mode(), mode);
        assert!(ui.dialog.is_none());
        assert_eq!(ui.editor.cursor(), FileOffset::new(1));
        key(&mut ui, KeyCode::F(4));
    }
    key(&mut ui, KeyCode::Esc);
    key(&mut ui, KeyCode::F(1));
    assert!(screen(&mut ui, 55, 12).contains("[Text]"));
    let help = screen(&mut ui, 80, 25);
    assert!(help.contains("F2       - Wrap   - Toggle line wrapping"));
    assert!(help.contains("F6       - LnFeed - Choose the line delimiter"));
    key(&mut ui, KeyCode::End);
    assert!(screen(&mut ui, 55, 12).contains("Exit this view"));
    key(&mut ui, KeyCode::Esc);
    assert!(ui.dialog.is_none());
    assert_eq!(ui.editor.cursor(), FileOffset::new(1));
}

#[test]
fn calculator_modal_evaluates_on_enter_and_restores_the_edit_context() {
    let mut ui = ui(&[0xFE, 0x80]);
    key(&mut ui, KeyCode::F(3));
    key(&mut ui, KeyCode::Char('A'));
    ui.key(KeyEvent::new(KeyCode::Char('='), KeyModifiers::ALT));

    let initial = screen(&mut ui, 80, 25);
    let lines: Vec<_> = initial.lines().collect();
    assert_eq!(lines[9].chars().nth(4), Some('╔'));
    assert!(lines[9].contains("═ Calculator ═"));
    assert!(lines[10].contains(&"░".repeat(68)));
    assert!(lines[13].contains(&format!("Bin:{:064b}", 0)));
    assert!(ui.has_input_caret());

    type_text(&mut ui, "1+2");
    assert!(screen(&mut ui, 80, 25).contains("Sig:0"));
    key(&mut ui, KeyCode::Enter);
    let evaluated = screen(&mut ui, 80, 25);
    assert!(evaluated.contains("Hex:00000000`00000003     \"       ♥\""));
    key(&mut ui, KeyCode::F(2));
    assert!(screen(&mut ui, 80, 25).contains("Sig:3"));
    key(&mut ui, KeyCode::F(1));
    assert!(!ui.has_input_caret());
    assert!(screen(&mut ui, 80, 25).contains("[Calculator]"));
    key(&mut ui, KeyCode::Esc);
    assert!(ui.has_input_caret());

    key(&mut ui, KeyCode::PageUp);
    type_text(&mut ui, "12");
    key(&mut ui, KeyCode::Home);
    type_text(&mut ui, "9");
    key(&mut ui, KeyCode::Insert);
    type_text(&mut ui, "8");
    key(&mut ui, KeyCode::Enter);
    assert!(screen(&mut ui, 80, 25).contains("Hex:00000000`00000982"));
    key(&mut ui, KeyCode::Esc);
    assert_eq!(ui.pending_nibble, Some(0xA));
    assert_eq!(bytes(&mut ui), [0xFE, 0x80]);
    assert_eq!(ui.editor.cursor(), FileOffset::new(0));

    control(&mut ui, KeyCode::Char('l'));
    assert!(screen(&mut ui, 80, 25).contains("982░"));
    key(&mut ui, KeyCode::PageUp);
    type_text(&mut ui, "7FFFFFFFFFFFFFFF+1");
    control(&mut ui, KeyCode::Char('e'));
    let checked = screen(&mut ui, 80, 25);
    assert_eq!(checked.lines().nth(11).unwrap().chars().nth(22), Some('^'));
    assert!(checked.contains("Error: Signed 64-bit arithmetic overflow"));
    key(&mut ui, KeyCode::Enter);
    assert!(screen(&mut ui, 80, 25).contains("Sig:-9223372036854775808"));
    key(&mut ui, KeyCode::PageUp);
    type_text(&mut ui, "1/0");
    key(&mut ui, KeyCode::Enter);
    let error = screen(&mut ui, 80, 25);
    assert_eq!(error.lines().nth(11).unwrap().chars().nth(9), Some('^'));
    assert!(error.contains("Error: Division by zero"));
    assert!(!error.contains("Bin:"));
    key(&mut ui, KeyCode::Esc);
    key(&mut ui, KeyCode::Char('5'));
    assert_eq!(bytes(&mut ui), [0xA5, 0x80]);
}

#[test]
fn calculator_endian_reads_preserve_pending_bytes_and_modal_state() {
    let original = [0xFE, 0x80, 1, 2];
    let mut ui = ui(&original);
    key(&mut ui, KeyCode::F(3));
    type_text(&mut ui, "A5");
    key(&mut ui, KeyCode::Left);
    key(&mut ui, KeyCode::Char('B'));
    let generation = ui.editor.document().generation();
    control(&mut ui, KeyCode::Char('l'));

    for (expression, expected) in [
        ("@W:le", 0x80A5_u64),
        ("@W:be", 0xA580),
        ("@w:be", (-0x5A80_i64) as u64),
        ("@D:BE", 0xA580_0102),
    ] {
        control(&mut ui, KeyCode::Char('u'));
        type_text(&mut ui, expression);
        key(&mut ui, KeyCode::Enter);
        let output = screen(&mut ui, 80, 25);
        assert!(output.contains(expression));
        assert!(output.contains(&format!(
            "Hex:{:08X}`{:08X}",
            expected >> 32,
            expected as u32
        )));
        assert!(output.contains("═ Calculator ═"));
    }
    for (expression, diagnostic, marker) in [
        ("@Q:be", "No value under cursor", 6),
        ("@W:middle", "Use :le or :be on a value tag", 8),
        ("@O:be", "Use :le or :be on a value tag", 8),
    ] {
        control(&mut ui, KeyCode::Char('u'));
        type_text(&mut ui, expression);
        key(&mut ui, KeyCode::Enter);
        let output = screen(&mut ui, 80, 25);
        assert!(output.contains(&format!("Error: {diagnostic}")));
        assert_eq!(
            output.lines().nth(11).unwrap().chars().nth(marker),
            Some('^')
        );
    }

    control(&mut ui, KeyCode::Char('u'));
    type_text(&mut ui, "@W:be");
    control(&mut ui, KeyCode::Char('e'));
    let checked = screen(&mut ui, 80, 25);
    assert!(checked.contains("Calculator [checked i64]"));
    assert!(checked.contains("Sig:42368"));
    key(&mut ui, KeyCode::F(1));
    assert!(!ui.has_input_caret());
    // Help can be scrolled without dispatching its keys to the expression.
    key(&mut ui, KeyCode::End);
    let help = screen(&mut ui, 80, 25);
    assert!(help.contains("@W:le / @W:be"));
    assert!(help.contains("Ctrl-E checks signed overflow (hiewbie)"));
    key(&mut ui, KeyCode::Esc);
    assert_eq!(screen(&mut ui, 80, 25), checked);
    let mut terminal = Terminal::new(TestBackend::new(80, 25)).unwrap();
    terminal.draw(|frame| ui.render(frame)).unwrap();
    terminal.backend_mut().assert_cursor_position((11, 10));

    key(&mut ui, KeyCode::Esc);
    assert_eq!(ui.pending_nibble, Some(0xB));
    assert_eq!(ui.editor.cursor(), FileOffset::new(0));
    assert_eq!(ui.editor.document().generation(), generation);
    assert_eq!(bytes(&mut ui), [0xA5, 0x80, 1, 2]);
    terminal.draw(|frame| ui.render(frame)).unwrap();
    terminal.backend_mut().assert_cursor_position((14, 1));

    control(&mut ui, KeyCode::Char('l'));
    assert_eq!(screen(&mut ui, 80, 25), checked);
    key(&mut ui, KeyCode::PageUp);
    assert!(screen(&mut ui, 80, 25).contains("Calculator [checked i64]"));
    type_text(&mut ui, "1");
    key(&mut ui, KeyCode::Enter);
    assert!(screen(&mut ui, 80, 25).contains("═ Calculator ═"));
    key(&mut ui, KeyCode::Esc);
    key(&mut ui, KeyCode::Char('6'));
    assert_eq!(bytes(&mut ui), [0xB6, 0x80, 1, 2]);
    control(&mut ui, KeyCode::Char('z'));
    assert_eq!(bytes(&mut ui), [0xA5, 0x80, 1, 2]);
    control(&mut ui, KeyCode::Char('y'));
    assert_eq!(bytes(&mut ui), [0xB6, 0x80, 1, 2]);
    key(&mut ui, KeyCode::Esc);
    assert_eq!(bytes(&mut ui), original);
}

#[test]
fn calculator_is_read_only_in_every_view_and_resizes_with_endian_input() {
    for mode in [ViewMode::Text, ViewMode::Hex, ViewMode::Code] {
        let mut ui = Ui::new(
            Editor::new(Document::new(MemorySource::new(vec![0x80, 0xFE])), true),
            "fixture.bin".into(),
        );
        ui.editor.dispatch(Command::SetMode(mode)).unwrap();
        control(&mut ui, KeyCode::Char('l'));
        type_text(&mut ui, "@w:be");
        key(&mut ui, KeyCode::Enter);
        for (width, height) in [(80, 25), (100, 30)] {
            let output = screen(&mut ui, width, height);
            assert!(output.contains("@w:be"));
            assert!(output.contains("Sig:-32514"));
            assert!(output.contains("Hex:FFFFFFFF`FFFF80FE"));
        }
        screen(&mut ui, 20, 4);
        screen(&mut ui, 1, 1);
        assert!(screen(&mut ui, 80, 25).contains("Sig:-32514"));
        key(&mut ui, KeyCode::Esc);
        assert_eq!(ui.editor.mode(), mode);
        assert_eq!(ui.editor.cursor(), FileOffset::new(0));
        assert_eq!(ui.editor.document().generation(), 0);
        assert!(!ui.editor.document().is_dirty());
        assert_eq!(bytes(&mut ui), [0x80, 0xFE]);
    }
}
