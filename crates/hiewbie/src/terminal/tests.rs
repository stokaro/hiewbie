use std::cell::Cell;
use std::io;
use std::rc::Rc;

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
    assert!(screen(&mut ui, 80, 25).contains("SELECT ISA"));
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
    assert!(screen(&mut ui, 80, 25).contains("[Hex editing]"));
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
fn raw_code_requires_an_explicit_profile_and_shows_its_context() {
    let mut ui = ui(&[0x90, 0xc3]);
    ui.dispatch(Command::SetMode(ViewMode::Code));
    assert!(screen(&mut ui, 80, 25).contains("SELECT ISA"));
    assert!(ui.message.contains("select an instruction architecture"));
    control(&mut ui, KeyCode::Char('a'));
    assert!(screen(&mut ui, 80, 25).contains("Decoder and raw addressing"));
    key(&mut ui, KeyCode::Char('3'));
    key(&mut ui, KeyCode::Enter);
    let output = screen(&mut ui, 80, 25);
    assert!(output.contains("CODE VIEW RW CLEAN x86-64 little RAW@0!"));
    assert!(output.contains("R:0000000000000000"));
    assert!(output.contains("nop"));
    assert!(output.contains("ret"));
    control(&mut ui, KeyCode::Char('b'));
    assert!(screen(&mut ui, 80, 25).contains("x86-16"));
    ui.key(KeyEvent::new(KeyCode::F(1), KeyModifiers::CONTROL));
    assert!(screen(&mut ui, 80, 25).contains("x86-32"));
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
    let output = screen(&mut ui, 80, 25);
    assert!(output.contains("V:0000000140001000 1> EB04"));
    assert!(output.contains("jmp"));
    assert!(output.contains("x86-64 little AUTO"));
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
    assert!(screen(&mut ui, 80, 25).contains("CODE EDIT RW CLEAN"));
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
    assert!(output.contains("CODE EDIT RW DIRTY"));
    assert!(output.contains("EB02"));
    assert!(output.contains("Asm-"));
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
    assert!(output.contains("CODE EDIT RW DIRTY SELECT ISA"));
    assert!(output.contains("Code unavailable; raw bytes:"));
    assert!(output.contains("0000000000000080  00"));
    assert!(ui.editor.is_editing());
    control(&mut ui, KeyCode::Char('z'));
    let output = screen(&mut ui, 80, 25);
    assert!(output.contains("x86-64 little AUTO"));
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
    assert!(output.contains("aarch64 big RAW@100000000!"));
    assert!(output.contains("unsupported"));
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
    assert!(screen(&mut ui, 80, 25).contains("FO 0000000100000010"));
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
    assert_eq!(cells[(31, 2)].symbol(), "E");
    for column in 31..41 {
        let bg = cells[(column, 2)].bg;
        assert_eq!(
            bg == Color::Magenta,
            (33..37).contains(&column),
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
    key(&mut ui, KeyCode::End);
    assert!(screen(&mut ui, 55, 12).contains("Close help with F1"));
    key(&mut ui, KeyCode::Esc);
    assert!(ui.dialog.is_none());
    assert_eq!(ui.editor.cursor(), FileOffset::new(1));
}
