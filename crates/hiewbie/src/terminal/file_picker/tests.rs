use std::time::{Duration, UNIX_EPOCH};

use ratatui::backend::TestBackend;
use ratatui::crossterm::event::KeyModifiers;

use super::*;

fn press(picker: &mut FilePicker, code: KeyCode) -> Action {
    picker.key(KeyEvent::new(code, KeyModifiers::NONE))
}

fn names(picker: &FilePicker) -> Vec<&str> {
    picker
        .order
        .iter()
        .map(|&index| picker.entries[index].name.to_str().unwrap())
        .collect()
}

#[test]
fn directory_changes_and_filter_preserve_native_names_and_selection() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path();
    fs::create_dir(path.join("alpha")).unwrap();
    fs::create_dir(path.join("zeta")).unwrap();
    fs::write(path.join("other.txt"), b"other bytes").unwrap();
    fs::write(path.join("alpha").join("inside.bin"), b"inside").unwrap();
    let file = path.join("space 中.bin");
    fs::write(&file, b"example").unwrap();

    let mut picker = FilePicker::new(path.into(), Some(&file));
    picker.reload();
    assert_eq!(
        names(&picker),
        ["..", "alpha", "zeta", "other.txt", "space 中.bin"]
    );
    assert_eq!(picker.selected_name(), file.file_name());

    press(&mut picker, KeyCode::F(10));
    press(&mut picker, KeyCode::PageUp);
    for character in "*.BIN".chars() {
        press(&mut picker, KeyCode::Char(character));
    }
    press(&mut picker, KeyCode::Enter);
    assert_eq!(names(&picker), ["..", "alpha", "zeta", "space 中.bin"]);
    assert_eq!(picker.selected_name(), file.file_name());

    press(&mut picker, KeyCode::Home);
    press(&mut picker, KeyCode::Down);
    press(&mut picker, KeyCode::Enter);
    assert_eq!(picker.directory, path.join("alpha"));
    assert_eq!(names(&picker), ["..", "inside.bin"]);
    assert_eq!(picker.selected_name(), Some(OsStr::new("..")));
    press(&mut picker, KeyCode::Enter);
    assert_eq!(picker.selected_name(), Some(OsStr::new("alpha")));

    press(&mut picker, KeyCode::F(8));
    assert_eq!(names(&picker), ["..", "zeta", "alpha", "space 中.bin"]);
    assert!(matches!(press(&mut picker, KeyCode::Esc), Action::Close));
    picker.reload();
    assert_eq!(picker.selected_name(), Some(OsStr::new("alpha")));
    press(&mut picker, KeyCode::End);
    assert!(matches!(press(&mut picker, KeyCode::Enter), Action::Open(picked) if picked == file));
    assert_eq!(fs::read(&file).unwrap(), b"example");
}

#[test]
fn file_picker_has_reference_frame_metadata_selection_and_filter_caret() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path();
    let file = path.join("space name.asm");
    fs::write(&file, b"abc").unwrap();
    let timestamp = UNIX_EPOCH + Duration::from_secs(1_776_428_430);
    fs::OpenOptions::new()
        .write(true)
        .open(&file)
        .unwrap()
        .set_modified(timestamp)
        .unwrap();
    let mut picker = FilePicker::new(path.into(), Some(&file));
    picker.reload();

    let mut terminal = Terminal::new(TestBackend::new(80, 25)).unwrap();
    terminal.draw(|frame| picker.render(frame)).unwrap();
    let buffer = terminal.backend().buffer();
    let row = |y| (0..80).map(|x| buffer[(x, y)].symbol()).collect::<String>();
    assert_eq!(buffer[(13, 3)].symbol(), "╔");
    assert_eq!(buffer[(66, 21)].symbol(), "╝");
    assert_eq!(buffer[(39, 21)].symbol(), "*");
    assert_eq!(buffer[(66, 2)].symbol(), "[");
    assert_eq!(buffer[(15, 5)].bg, Color::Black);
    assert_eq!(buffer[(15, 4)].bg, Color::Cyan);
    assert!(row(4).contains("│>UP---DIR<│Attr│---Date---│--Time--"));
    let (date, time) = timestamp::columns(&fs::metadata(&file).unwrap());
    assert!(row(5).contains(&format!("space name.asm│         3│.a..│{date}│{time}")));

    press(&mut picker, KeyCode::F(10));
    terminal.draw(|frame| picker.render(frame)).unwrap();
    assert_eq!(terminal.get_cursor_position().unwrap(), (21, 12).into());
    assert_eq!(terminal.backend().buffer()[(18, 11)].symbol(), "╔");
    assert_eq!(terminal.backend().buffer()[(20, 12)].symbol(), "*");
    assert_eq!(terminal.backend().buffer()[(21, 12)].symbol(), "░");
    press(&mut picker, KeyCode::F(1));
    terminal.draw(|frame| picker.render(frame)).unwrap();
    assert!(
        (5..75)
            .map(|x| terminal.backend().buffer()[(x, 5)].symbol())
            .collect::<String>()
            .contains("[GetStringPattern]")
    );
    press(&mut picker, KeyCode::Esc);
    terminal.draw(|frame| picker.render(frame)).unwrap();
    assert_eq!(terminal.get_cursor_position().unwrap(), (21, 12).into());
    press(&mut picker, KeyCode::Backspace);
    press(&mut picker, KeyCode::Char('z'));
    press(&mut picker, KeyCode::Esc);
    assert!(picker.filter_input.is_none());
    assert_eq!(picker.filter, "*");
    assert_eq!(picker.selected_name(), file.file_name());

    let mut narrow = Terminal::new(TestBackend::new(18, 4)).unwrap();
    narrow.draw(|frame| picker.render(frame)).unwrap();
}

#[test]
fn failed_directory_lookup_keeps_the_current_listing() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path();
    let directory = path.join("vanished");
    fs::create_dir(&directory).unwrap();
    let mut picker = FilePicker::new(path.into(), Some(&directory));
    picker.reload();
    fs::remove_dir(&directory).unwrap();
    assert!(matches!(
        press(&mut picker, KeyCode::Enter),
        Action::Continue
    ));
    assert!(picker.error.as_deref().unwrap().contains("vanished"));
    assert_eq!(picker.directory, path);
    assert_eq!(picker.selected_name(), Some(OsStr::new("vanished")));
    press(&mut picker, KeyCode::Esc);
    assert!(picker.error.is_none());
}

#[test]
fn wildcard_filter_matches_full_names_and_can_retry_a_star() {
    for (pattern, name, expect) in [
        ("*", "", true),
        ("a*b?d", "ababcD", true),
        ("*aaba", "aaaaba", true),
        ("*.BIN", "A.bin", true),
        ("a?c", "a中c", true),
        ("x*", "ax", false),
        ("*.bin", "thing.bin.tmp", false),
        ("?", "ab", false),
        ("a*b", "a", false),
    ] {
        assert_eq!(filter::matches(pattern, name), expect, "{pattern} {name}");
    }
}
