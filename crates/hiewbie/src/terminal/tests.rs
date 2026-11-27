use std::cell::Cell;
use std::io;
use std::rc::Rc;

use hiewbie_core::{Document, MemorySource};
use ratatui::backend::TestBackend;

use super::*;

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
        format!(" hiewbie {} | fixture.bin", env!("CARGO_PKG_VERSION")),
        "0000000000000000 / 0000000000000011  HEX VIEW RW CLEAN".into(),
        "0000000000000000  4865 6C6C 6F00 776F 726C 6421 0AFF 5859  Hello.world!..XY".into(),
        format!("{:<59}Z", "0000000000000010  5A"),
        String::new(),
        String::new(),
        "F1 help | Enter/F4 Hex/Text | Esc quit | Code mode planned".into(),
        " 1Help   2--     3Edit   4Mode   5Goto   6Refs-  7Search 8Hdr-   9Open- 10--".into(),
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
    assert!(ui.message.contains("byte restore is not implemented"));
    assert_eq!(key(&mut ui, KeyCode::Esc), UiAction::Continue);
    assert!(!ui.editor.is_editing());
    assert_eq!(bytes(&mut ui), [0x11, 0x22, 0x33]);
    assert_eq!(key(&mut ui, KeyCode::Esc), UiAction::Exit);
}

#[test]
fn contextual_save_ascii_undo_and_redo_use_shared_commands() {
    let mut ui = ui(b"ABC");
    assert_eq!(key(&mut ui, KeyCode::F(9)), UiAction::Continue);
    assert!(ui.message.contains("F9 Open"));
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
fn modes_share_cursor_and_read_only_cannot_begin_editing() {
    let mut ui = ui(b"0123456789");
    key(&mut ui, KeyCode::Right);
    key(&mut ui, KeyCode::Enter);
    assert_eq!(ui.editor.mode(), ViewMode::Text);
    assert_eq!(ui.editor.cursor(), FileOffset::new(1));
    key(&mut ui, KeyCode::F(4));
    assert_eq!(ui.editor.mode(), ViewMode::Hex);
    assert_eq!(ui.editor.cursor(), FileOffset::new(1));
    let editor = Editor::new(Document::new(MemorySource::new(b"ABC".to_vec())), true);
    let mut read_only = Ui::new(editor, "read-only".into());
    key(&mut read_only, KeyCode::F(3));
    type_text(&mut read_only, "ff");
    assert!(!read_only.editor.is_editing());
    assert_eq!(bytes(&mut read_only), b"ABC");
    assert!(screen(&mut read_only, 80, 25).contains("VIEW RO CLEAN"));
}

#[test]
fn help_and_paste_cannot_execute_or_discard_edits() {
    let mut ui = ui(b"ABC");
    key(&mut ui, KeyCode::F(3));
    type_text(&mut ui, "44");
    key(&mut ui, KeyCode::F(1));
    assert!(screen(&mut ui, 80, 25).contains("hiewbie help"));
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
fn literal_search_repeats_and_long_scan_can_be_cancelled() {
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
    assert!(ui.search.is_none());
    assert!(ui.message.contains("No further match"));

    let mut long = self::ui(&vec![0; SEARCH_STEP_BYTES * 3]);
    long.start_search(vec![1, 2, 3], FileOffset::new(0))
        .unwrap();
    long.advance_search();
    assert!(long.search.is_some());
    assert_eq!(key(&mut long, KeyCode::Esc), UiAction::Continue);
    assert!(long.search.is_none());
    assert_eq!(long.editor.cursor(), FileOffset::new(0));
    assert!(long.message.contains("cancelled"));
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
    assert!(output.contains("\\u{1b}[31m\\n\\u{202e}file"));
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
    assert!(output.contains("0000000100000010"));
    assert!(largest.get() <= 16 * 21);
    ui.dispatch(Command::SetMode(ViewMode::Text));
    screen(&mut ui, 300, 300);
    assert!(largest.get() <= hiewbie_core::MAX_READ_BYTES);
    ui.dispatch(Command::Goto(FileOffset::new(u64::MAX - 1)));
    let output = screen(&mut ui, 80, 25);
    assert!(output.contains("FFFFFFFFFFFFFFFE"));
}
