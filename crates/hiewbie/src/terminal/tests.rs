use std::cell::Cell;
use std::io;
use std::rc::Rc;

use hiewbie_core::{Document, MemorySource};
use ratatui::backend::TestBackend;

use super::*;

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
        format!(" hiewbie {} | fixture.bin", env!("CARGO_PKG_VERSION")),
        "0000000000000000 / 0000000000000011  HEX VIEW RW CLEAN".into(),
        "0000000000000000  4865 6C6C 6F00 776F 726C 6421 0AFF 5859  Hello.world!..XY".into(),
        format!("{:<59}Z", "0000000000000010  5A"),
        String::new(),
        String::new(),
        "F1 help | Enter cycle | F4 select mode | F8 headers | Esc quit".into(),
        " 1Help   2--     3Edit   4Mode   5Goto   6Refs-  7Search 8Header 9Open- 10--".into(),
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
    assert_eq!(ui.editor.mode(), ViewMode::Text);
    assert_eq!(ui.editor.cursor(), FileOffset::new(1));
    key(&mut ui, KeyCode::F(4));
    assert_eq!(ui.editor.mode(), ViewMode::Text);
    key(&mut ui, KeyCode::Down);
    key(&mut ui, KeyCode::Enter);
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
        .skip(9)
        .take(7)
        .map(|line| line.chars().skip(23).take(34).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(
        popup,
        [
            format!("┌ View mode {}┐", "─".repeat(21)),
            format!("│{:<32}│", "  Text"),
            format!("│{:<32}│", "> Hex"),
            format!("│{:<32}│", "  Code"),
            format!("│{:<32}│", ""),
            format!("│{:<32}│", "Enter selects; Esc cancels"),
            format!("└{}┘", "─".repeat(32)),
        ]
        .join("\n")
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
    assert_eq!(ui.editor.mode(), ViewMode::Code);
    key(&mut ui, KeyCode::Enter);
    assert_eq!(ui.editor.mode(), ViewMode::Hex);

    key(&mut ui, KeyCode::F(4));
    for (width, height) in [(23, 5), (40, 10), (80, 25)] {
        let output = screen(&mut ui, width, height);
        assert!(output.contains("View mode"));
        assert!(output.contains("Code"));
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
fn shift_f7_repeats_search_only_from_view_without_modifier_collisions() {
    let mut ui = ui(b"a--a--a");
    let repeat = KeyEvent::new(KeyCode::F(7), KeyModifiers::SHIFT);
    ui.key(repeat);
    assert!(ui.message.contains("No previous search"));
    assert!(ui.dialog.is_none());
    key(&mut ui, KeyCode::F(7));
    type_text(&mut ui, "61");
    ui.key(repeat);
    assert!(matches!(ui.dialog, Some(Dialog::Input(_))));
    assert!(ui.search.is_none());
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
    assert!(ui.search.is_none());
    assert_eq!(ui.editor.cursor(), FileOffset::new(3));
    key(&mut ui, KeyCode::F(3));
    ui.key(repeat);
    assert!(ui.search.is_none());
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
