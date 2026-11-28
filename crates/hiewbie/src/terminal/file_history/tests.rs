use std::fs;

use ratatui::backend::TestBackend;

use super::*;
use crate::terminal::UiAction;

fn fixture(path: &Path, name: &str, data: &[u8]) -> PathBuf {
    let path = path.join(name);
    fs::write(&path, data).unwrap();
    std::path::absolute(path).unwrap()
}

fn open(path: &Path, read_only: bool) -> Ui<FileSource> {
    ui_for_picked_file(FileSource::open(path).unwrap(), read_only)
}

#[test]
fn tab_preserves_history_order_and_restores_the_document_position() {
    let root = tempfile::tempdir().unwrap();
    let a = fixture(root.path(), "a.bin", &[1; 0x300]);
    let b = fixture(root.path(), "b.bin", &[2; 0x300]);
    let c = fixture(root.path(), "c.bin", &[3; 0x300]);
    let mut ui = open(&a, true);
    ui.dispatch(Command::SetMode(ViewMode::Hex));
    ui.dispatch(Command::Goto(FileOffset::new(0x221)));
    ui.top = 0x200;
    let mut files = FileSession::new(&ui, Some(&a), vec![b.clone(), c.clone()], true);

    files.advance(&mut ui, FileNavigation::NextArgument);
    assert_eq!(active_path(&ui), b);
    assert_eq!(ui.editor.mode(), ViewMode::Text);
    assert_eq!(ui.editor.cursor().get(), 0);
    files.advance(&mut ui, FileNavigation::NextArgument);
    assert_eq!(active_path(&ui), c);
    assert!(ui.editor.is_read_only());

    files.advance(&mut ui, FileNavigation::NextRecent);
    assert_eq!(active_path(&ui), b);
    files.advance(&mut ui, FileNavigation::NextRecent);
    assert_eq!(active_path(&ui), a);
    assert_eq!(ui.editor.mode(), ViewMode::Hex);
    assert_eq!(ui.editor.cursor().get(), 0x221);
    assert_eq!(ui.top, 0x200);
    assert_eq!(
        files
            .recent
            .iter()
            .map(|entry| entry.path.as_path())
            .collect::<Vec<_>>(),
        [c.as_path(), b.as_path(), a.as_path()]
    );
    let mut bytes = [0];
    ui.editor
        .document_mut()
        .read_at(FileOffset::new(0), &mut bytes)
        .unwrap();
    assert_eq!(bytes, [1]);
    files.advance(&mut ui, FileNavigation::NextRecent);
    assert_eq!(active_path(&ui), c);
}

#[test]
fn reopening_the_active_file_from_files_refreshes_the_source() {
    let root = tempfile::tempdir().unwrap();
    let path = fixture(root.path(), "active.bin", &[1, 2]);
    let replacement = fixture(root.path(), "replacement.bin", &[3, 4, 5]);
    let mut ui = open(&path, false);
    ui.dispatch(Command::SetMode(ViewMode::Hex));
    ui.dispatch(Command::Goto(FileOffset::new(1)));
    let mut files = FileSession::new(&ui, Some(&path), vec![], false);

    fs::rename(replacement, &path).unwrap();
    files.open_source(&mut ui, FileSource::open(&path).unwrap());

    let mut data = [0; 3];
    ui.editor
        .document_mut()
        .read_at(FileOffset::new(0), &mut data)
        .unwrap();
    assert_eq!(data, [3, 4, 5]);
    assert_eq!(ui.editor.mode(), ViewMode::Hex);
    assert_eq!(ui.editor.cursor().get(), 1);
    assert_eq!(files.recent.len(), 1);
}

#[test]
fn argument_navigation_wraps_and_counts_duplicate_slots_independently() {
    let root = tempfile::tempdir().unwrap();
    let a = fixture(root.path(), "a.bin", &[1]);
    let b = fixture(root.path(), "b.bin", &[2]);
    let mut ui = open(&a, false);
    let mut files = FileSession::new(&ui, Some(&a), vec![b.clone(), a.clone()], false);

    for (navigation, path, index) in [
        (FileNavigation::PreviousArgument, &a, 2),
        (FileNavigation::NextArgument, &a, 0),
        (FileNavigation::NextArgument, &b, 1),
        (FileNavigation::NextArgument, &a, 2),
        (FileNavigation::PreviousArgument, &b, 1),
    ] {
        files.advance(&mut ui, navigation);
        assert_eq!(active_path(&ui), path);
        assert_eq!(files.argument_index, Some(index));
    }

    fs::remove_file(&a).unwrap();
    files.advance(&mut ui, FileNavigation::NextArgument);
    assert_eq!(active_path(&ui), b);
    assert_eq!(files.argument_index, Some(1));
    assert!(ui.message.contains("Cannot open"));

    let mut arguments = List::new(ListKind::Arguments, 0);
    assert!(matches!(
        arguments.key(
            &mut files,
            &b,
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)
        ),
        ListAction::Close
    ));
    assert_eq!(active_path(&ui), b);
    assert_eq!(files.argument_index, Some(0));
    files.advance(&mut ui, FileNavigation::NextArgument);
    assert_eq!(active_path(&ui), b);
    assert_eq!(files.argument_index, Some(1));
}

#[test]
fn mark_filters_tab_and_deleting_history_leaves_files_untouched() {
    let root = tempfile::tempdir().unwrap();
    let a = fixture(root.path(), "a.bin", &[1]);
    let b = fixture(root.path(), "b.bin", &[2]);
    let mut ui = open(&a, false);
    let mut files = FileSession::new(&ui, Some(&a), vec![b.clone()], false);
    files.advance(&mut ui, FileNavigation::NextArgument);
    assert_eq!(active_path(&ui), b);

    let mut list = List::new(ListKind::Recent, 1);
    list.key(
        &mut files,
        &b,
        KeyEvent::new(KeyCode::F(5), KeyModifiers::NONE),
    );
    assert!(files.recent[1].marked);
    files.advance(&mut ui, FileNavigation::NextRecent);
    assert_eq!(active_path(&ui), a);
    files.advance(&mut ui, FileNavigation::NextRecent);
    assert_eq!(active_path(&ui), a);

    list.key(
        &mut files,
        &a,
        KeyEvent::new(KeyCode::F(8), KeyModifiers::NONE),
    );
    assert_eq!(files.recent.len(), 2);
    list.selected = 0;
    list.key(
        &mut files,
        &a,
        KeyEvent::new(KeyCode::F(8), KeyModifiers::NONE),
    );
    assert_eq!(files.recent.len(), 1);
    assert_eq!(fs::read(b).unwrap(), [2]);
}

#[test]
fn history_and_arguments_have_their_own_reference_geometry() {
    let root = tempfile::tempdir().unwrap();
    let file = fixture(root.path(), "a.bin", &[1]);
    let ui = open(&file, false);
    let mut files = FileSession::new(&ui, Some(Path::new("a.bin")), vec!["b.bin".into()], false);
    files.full_paths = false;
    let mut history = List::new(ListKind::Recent, 0);
    let mut terminal = Terminal::new(TestBackend::new(80, 25)).unwrap();
    terminal
        .draw(|frame| {
            frame.render_widget(
                Paragraph::new("A").style(Style::new().fg(Color::Cyan).bg(Color::Blue)),
                Rect::new(76, 5, 1, 1),
            );
            history.render(frame, &files);
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(6, 3)].symbol(), "╔");
    assert_eq!(buffer[(73, 21)].symbol(), "╝");
    assert_eq!(buffer[(8, 4)].bg, Color::Black);
    assert_eq!(buffer[(76, 5)].symbol(), "A");
    assert_eq!(buffer[(76, 5)].fg, Color::Cyan);
    assert_eq!(buffer[(76, 5)].bg, Color::Black);
    let row_text = |buffer: &ratatui::buffer::Buffer, y| {
        (0..80).map(|x| buffer[(x, y)].symbol()).collect::<String>()
    };
    assert!(row_text(buffer, 3).contains("═ Mode  Offset ═"));
    assert!(row_text(buffer, 4).contains(" Text│00000000│a.bin"));
    assert!(row_text(buffer, 24).contains("8Delete 9Files"));

    let mut arguments = List::new(ListKind::Arguments, 0);
    terminal
        .draw(|frame| arguments.render(frame, &files))
        .unwrap();
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(32, 3)].symbol(), "╔");
    assert_eq!(buffer[(47, 21)].symbol(), "╝");
    assert_eq!(buffer[(34, 4)].bg, Color::Black);
    assert!(row_text(buffer, 4).contains("║ a.bin"));
    assert!(row_text(buffer, 21).contains("2 files"));
    assert!(!row_text(buffer, 24).contains("Help"));

    history.help = Some(Help::new(HelpTopic::HistoryFiles));
    terminal
        .draw(|frame| history.render(frame, &files))
        .unwrap();
    let buffer = terminal.backend().buffer();
    assert!(row_text(buffer, 8).contains("[HistoryFiles]"));
    assert!(row_text(buffer, 16).contains("Ctrl-Enter"));
    assert_eq!(buffer[(8, 9)].bg, Color::Black);
}

#[test]
fn file_navigation_keys_are_contextual() {
    let root = tempfile::tempdir().unwrap();
    let file = fixture(root.path(), "a.bin", &[1]);
    let mut ui = open(&file, false);
    for (key, modifiers, operation) in [
        (KeyCode::Tab, KeyModifiers::NONE, FileNavigation::NextRecent),
        (
            KeyCode::Backspace,
            KeyModifiers::CONTROL,
            FileNavigation::History,
        ),
        (
            KeyCode::Char('h'),
            KeyModifiers::CONTROL,
            FileNavigation::History,
        ),
        (
            KeyCode::F(9),
            KeyModifiers::CONTROL,
            FileNavigation::Arguments,
        ),
        (
            KeyCode::F(11),
            KeyModifiers::CONTROL,
            FileNavigation::PreviousArgument,
        ),
        (
            KeyCode::F(12),
            KeyModifiers::CONTROL,
            FileNavigation::NextArgument,
        ),
    ] {
        assert_eq!(
            ui.key(KeyEvent::new(key, modifiers)),
            UiAction::NavigateFile(operation)
        );
    }
    ui.dispatch(Command::BeginEdit);
    assert_eq!(
        ui.key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
        UiAction::Continue
    );
    assert!(ui.editor.is_editing());
}
