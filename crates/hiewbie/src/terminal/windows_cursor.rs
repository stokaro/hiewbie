use winconsole::console::{get_cursor_size, set_cursor_size};

const HIEW_CURSOR_PERCENT: u8 = 12;

pub(super) struct ConsoleCursor {
    original_size: u8,
}

impl ConsoleCursor {
    pub(super) fn underline() -> Option<Self> {
        let original_size = get_cursor_size().ok()?;
        set_cursor_size(HIEW_CURSOR_PERCENT).ok()?;
        // Some alternate buffers accept the native request while retaining the old size.
        if get_cursor_size().ok() != Some(HIEW_CURSOR_PERCENT) {
            let _ = set_cursor_size(original_size);
            return None;
        }
        Some(Self { original_size })
    }
}

impl Drop for ConsoleCursor {
    fn drop(&mut self) {
        let _ = set_cursor_size(self.original_size);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::{ActiveCaret, TerminalGuard};

    #[test]
    #[ignore = "requires an attached Windows console"]
    fn restores_the_previous_console_cursor_height() {
        let original = get_cursor_size().expect("read the primary console cursor height");
        {
            let _cursor =
                ConsoleCursor::underline().expect("set the primary console cursor height");
            assert_eq!(get_cursor_size().unwrap(), HIEW_CURSOR_PERCENT);
        }
        assert_eq!(get_cursor_size().unwrap(), original);

        let mut terminal = TerminalGuard::enter().expect("attach to the Windows console");
        let alternate_original = get_cursor_size().expect("read the alternate cursor height");
        terminal.set_input_caret(true).unwrap();
        let expected_size = match terminal.caret.as_ref().unwrap() {
            ActiveCaret::Native(_) => HIEW_CURSOR_PERCENT,
            ActiveCaret::Ansi => alternate_original,
        };
        assert_eq!(get_cursor_size().unwrap(), expected_size);
        terminal.set_input_caret(false).unwrap();
        assert_eq!(get_cursor_size().unwrap(), alternate_original);
        drop(terminal);
        assert_eq!(get_cursor_size().unwrap(), original);
    }
}
