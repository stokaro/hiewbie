use hiewbie_core::ViewMode;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::is_control_char;
use super::render::{dialog_surface, dialog_surface_with_tinted_shadow};

const PANEL: Style = Style::new().fg(Color::Black).bg(Color::Green);
const BODY: Style = PANEL.fg(Color::LightYellow);
const SELECTED: Style = Style::new().fg(Color::White).bg(Color::Black);

#[derive(Clone, Copy, Debug)]
pub(super) enum HelpTopic {
    Files,
    HistoryFiles,
    FileFilter,
    View(ViewMode),
    Edit(ViewMode),
    Search,
    Calculator,
}

impl HelpTopic {
    fn title(self) -> &'static str {
        match self {
            Self::Files => "[FileList]",
            Self::HistoryFiles => "[HistoryFiles]",
            Self::FileFilter => "[GetStringPattern]",
            Self::View(ViewMode::Text) => "[Text]",
            Self::View(ViewMode::Hex) => "[Hex]",
            Self::View(ViewMode::Code) => "[Code]",
            Self::Edit(ViewMode::Text) => "[TextEdit]",
            Self::Edit(ViewMode::Hex) => "[HexEdit]",
            Self::Edit(ViewMode::Code) => "[CodeEdit]",
            Self::Search => "[FindEdit]",
            Self::Calculator => "[Calculator]",
        }
    }

    fn sections(self) -> [&'static [Entry]; 3] {
        match self {
            Self::Files => [FILE_PICKER, &[], &[]],
            Self::HistoryFiles => [FILE_HISTORY_HELP, &[], &[]],
            Self::FileFilter => [FILE_FILTER, &[], &[]],
            Self::View(ViewMode::Text) => [TEXT, TEXT_MORE, VIEW_FALLBACKS],
            Self::View(ViewMode::Hex) => [HEX, HEX_MORE, VIEW_FALLBACKS],
            Self::View(ViewMode::Code) => [CODE, CODE_MORE, VIEW_FALLBACKS],
            Self::Edit(ViewMode::Text | ViewMode::Hex) => [HEX_EDIT, &[], &[]],
            Self::Edit(ViewMode::Code) => [CODE_EDIT, &[], &[]],
            Self::Search => [FIND, &[], &[]],
            Self::Calculator => [CALCULATOR_HELP, &[], &[]],
        }
    }
}

#[derive(Debug)]
pub(super) struct Help {
    topic: HelpTopic,
    selected: usize,
    first: usize,
    rows: usize,
}

impl Help {
    pub(super) fn new(topic: HelpTopic) -> Self {
        Self {
            topic,
            selected: 0,
            first: 0,
            rows: 1,
        }
    }

    fn len(&self) -> usize {
        self.topic
            .sections()
            .iter()
            .map(|section| section.len())
            .sum()
    }

    // A Help row is an index entry. Closing or moving the highlight has no
    // effect on the command or pending input behind the dialog.
    pub(super) fn closes(&mut self, key: KeyEvent) -> bool {
        if is_control_char(key, 'c') {
            return true;
        }
        if !key.modifiers.is_empty() {
            return false;
        }
        let last = self.len() - 1;
        match key.code {
            KeyCode::Esc | KeyCode::Enter => return true,
            KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down => self.selected = (self.selected + 1).min(last),
            KeyCode::Home => self.selected = 0,
            KeyCode::End => self.selected = last,
            KeyCode::PageUp => self.selected = self.first.saturating_sub(self.rows),
            KeyCode::PageDown => self.selected = (self.first + 2 * self.rows - 1).min(last),
            _ => {}
        }
        self.keep_selection_visible(last + 1);
        false
    }

    fn keep_selection_visible(&mut self, len: usize) {
        let first_visible = self.selected.saturating_sub(self.rows - 1);
        self.first = self.first.clamp(first_visible, self.selected);
        self.first = self.first.min(len.saturating_sub(self.rows));
    }

    pub(super) fn render(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let len = self.len();
        let height = (len as u16 + 4).min(area.height.saturating_sub(4));
        // Center on the lower middle row for an even console height.
        let center = Rect::new(
            area.x,
            area.y.saturating_add(1),
            area.width,
            area.height.saturating_sub(1),
        );
        let width = area.width.saturating_sub(8);
        let height = height.max(5).min(center.height);
        let title = Line::from(self.topic.title()).centered();
        let content = if matches!(self.topic, HelpTopic::HistoryFiles) {
            dialog_surface_with_tinted_shadow(frame, center, width, height, title, PANEL)
        } else {
            dialog_surface(frame, center, width, height, title, PANEL)
        };
        self.rows = usize::from(content.height).max(1);
        self.keep_selection_visible(len);

        let highlight = content.inner(Margin::new(1, 0));
        let calculator = matches!(self.topic, HelpTopic::Calculator);
        for (index, entry) in self
            .topic
            .sections()
            .into_iter()
            .flatten()
            .enumerate()
            .skip(self.first)
            .take(usize::from(content.height))
        {
            let y = content.y + (index - self.first) as u16;
            let selected = index == self.selected;
            if selected {
                frame.render_widget(
                    Paragraph::new("").style(SELECTED),
                    Rect::new(highlight.x, y, highlight.width, 1),
                );
            }
            let x = highlight
                .x
                .saturating_add(u16::from(!calculator))
                .min(content.right());
            let edge = if selected {
                highlight.right()
            } else {
                content.right()
            };
            let width = edge.saturating_sub(x);
            if width == 0 {
                continue;
            }
            let style = if selected { SELECTED } else { BODY };
            let mut text = entry.text();
            let line = if text.chars().count() > usize::from(width) {
                text = text.chars().take(usize::from(width) - 1).collect();
                Line::from(vec![
                    Span::styled(text, style),
                    Span::styled("►", if selected { SELECTED } else { PANEL }),
                ])
            } else {
                Line::styled(text, style)
            };
            frame.render_widget(Paragraph::new(line).style(style), Rect::new(x, y, width, 1));
        }
    }
}

#[derive(Clone, Copy)]
enum Entry {
    Key(&'static str, &'static str),
    Function(&'static str, &'static str, &'static str),
    Section(usize, &'static str),
    Text(&'static str),
}

const fn key(binding: &'static str, description: &'static str) -> Entry {
    Entry::Key(binding, description)
}

const fn function(binding: &'static str, label: &'static str, description: &'static str) -> Entry {
    Entry::Function(binding, label, description)
}

const fn section(lead: usize, title: &'static str) -> Entry {
    Entry::Section(lead, title)
}

impl Entry {
    fn text(self) -> String {
        match self {
            Self::Key(binding, description) => format!("{binding:<17} - {description}"),
            Self::Function(binding, label, description) => {
                format!("{binding:<8} - {label:<6} - {description}")
            }
            Self::Text(text) => text.to_owned(),
            Self::Section(lead, title) => {
                let tail = 60 - lead - title.len();
                format!("{} {title} {}", "─".repeat(lead), "─".repeat(tail))
            }
        }
    }
}

const ESCAPE: Entry = key("Escape", "Exit the current file");
const FILE_HISTORY: Entry = key("Ctrl-Backspace", "Browse file history");
const NEXT_FILE: Entry = key("Tab", "Switch to the next recent file");
const SCREEN: Entry = key("Alt-P", "Export the screen (planned)");
const CALCULATOR: Entry = key("Alt-=", "Open the calculator");
const INFORMATION: Entry = key("Ctrl-Alt", "Show file information (planned)");
const REPEAT: Entry = key("Ctrl-Enter", "Repeat the last search");
const COMMENT: Entry = key("Semicolon", "Annotate this offset (planned)");
const FUNCTIONS: Entry = section(20, "Functions keys");
const CURSORS: Entry = section(21, "Cursors movement");
const WRITE_BLOCK: Entry = function("F2", "PutBlk", "Export the marked range");
const EDIT: Entry = function("F3", "Edit", "Start a byte edit");
const MODE: Entry = function("F4", "Mode", "Choose a view");
const GOTO: Entry = function("F5", "Goto", "Open the address prompt");
const SEARCH: Entry = function("F7", "Search", "Open the pattern prompt");
const HEADER: Entry = function("F8", "Header", "Inspect the executable");
const FILES: Entry = function("F9", "Files", "Browse files");
const QUIT: Entry = function(
    "F10",
    "Quit",
    "Exit while preserving the timestamp (planned)",
);
const MODULES: Entry = function("F11", "Hem", "Manage extensions (planned)");
const NAMES: Entry = function("F12", "Names", "Manage offset names (planned)");
const GLOBAL: Entry = function("Alt-F1", "Global", "Change address display (planned)");
const FILL: Entry = function("Alt-F2", "FilBlk", "Fill the marked range (planned)");
const RELOAD: Entry = function("Alt-F4", "ReLoad", "Load the file again (planned)");
const STRINGS: Entry = function("Alt-F6", "String", "List readable strings (planned)");
const DIRECTION: Entry = function("Alt-F7", "Direct", "Reverse search direction (planned)");
const TABLE: Entry = function("Alt-F8", "Table", "Select a character table (planned)");
const LEAVE: Entry = function("Alt-F10", "Leave", "Close the file (planned)");
const ADD_NAME: Entry = function("Alt-F12", "AddNam", "Name an address (planned)");
const IMPORT: Entry = function("Ctrl-F2", "GetBlk", "Import bytes from a file (planned)");
const REPLACE: Entry = function("Ctrl-F3", "Replac", "Replace matching bytes (planned)");
const REREAD: Entry = function("Ctrl-F4", "ReRead", "Reload the current buffer (planned)");
const BASE: Entry = function("Ctrl-F5", "Base", "Set an address base (planned)");
const OTHER: Entry = function("Ctrl-F7", "Other", "Find a different byte (planned)");
const ARGUMENTS: Entry = function("Ctrl-F9", "FilArg", "List the command-line files");
const SAVE_STATE: Entry = function("Ctrl-F10", "SavSta", "Store the file session (planned)");
const PREV_FILE: Entry = function(
    "Ctrl-F11",
    "PrvFil",
    "Open the preceding argument (planned)",
);
const FOLLOWING_FILE: Entry = function(
    "Ctrl-F12",
    "NxtFil",
    "Open the following argument (planned)",
);
const NEXT_SEARCH: Entry = function("Shift-F7", "NextSr", "Repeat in the same direction");
const FIRST_BYTE: Entry = key("Ctrl-Home", "Go to the first file byte");
const LAST_BYTE: Entry = key("Ctrl-End", "Go to the last file byte");
const BITNESS: Entry = function("Ctrl-F1", "OpSize", "Cycle x86 decoding width");

const HEX: &[Entry] = &[
    key("Enter", "Switch to Code"),
    ESCAPE,
    NEXT_FILE,
    SCREEN,
    CALCULATOR,
    INFORMATION,
    FILE_HISTORY,
    REPEAT,
    COMMENT,
    FUNCTIONS,
    WRITE_BLOCK,
    EDIT,
    MODE,
    GOTO,
    function("F6", "DatRef", "Find a data reference (planned)"),
    SEARCH,
    HEADER,
    FILES,
    QUIT,
    MODULES,
    NAMES,
    GLOBAL,
    FILL,
    function("Alt-F3", "CryBlk", "Transform a block (planned)"),
    RELOAD,
    STRINGS,
    DIRECTION,
    TABLE,
    LEAVE,
    ADD_NAME,
    IMPORT,
    REPLACE,
    REREAD,
    BASE,
    OTHER,
    ARGUMENTS,
    SAVE_STATE,
    PREV_FILE,
    FOLLOWING_FILE,
    function("Shift-F2", "DelBlk", "Remove the marked range (planned)"),
    function("Shift-F3", "Insert", "Insert at the current byte (planned)"),
    function(
        "Shift-F4",
        "PrtBlk",
        "Export a text dump of the block (planned)",
    ),
];

const HEX_MORE: &[Entry] = &[
    function("Shift-F5", "CpyBlk", "Duplicate the marked bytes (planned)"),
    function("Shift-F6", "MovBlk", "Relocate the marked bytes (planned)"),
    NEXT_SEARCH,
    function("Shift-F9", "OffTbl", "List saved offsets (planned)"),
    function("Shift-F10", "GoCurs", "Jump to a saved offset (planned)"),
    function("Shift-F11", "Jump", "Apply a relative offset (planned)"),
    function("Shift-F12", "AddNam", "Name the current offset (planned)"),
    key("Alt-M", "Color a marked range (planned)"),
    key("Alt-N", "Go to the next color mark (planned)"),
    CURSORS,
    key("Ctrl-Right", "Shift the screen right (planned)"),
    key("Ctrl-Left", "Shift the screen left (planned)"),
    FIRST_BYTE,
    LAST_BYTE,
    key("Ctrl-PgDn", "Scroll one screen row up (planned)"),
    key("Ctrl-PgUp", "Scroll one screen row down (planned)"),
    section(26, "Block"),
    key("*", "Start, finish, or clear the mark"),
    key("Ctrl-* / Ctrl-K", "Mark every byte"),
    key("Alt-*", "Adjust the marked range"),
    key("[ / ]", "Go to the start or end of the block"),
];

const TEXT: &[Entry] = &[
    key("Enter", "Switch to Hex"),
    NEXT_FILE,
    ESCAPE,
    SCREEN,
    CALCULATOR,
    INFORMATION,
    FILE_HISTORY,
    REPEAT,
    section(20, "Function  keys"),
    function("F2", "Wrap", "Toggle line wrapping"),
    MODE,
    GOTO,
    function("F6", "LnFeed", "Choose the line delimiter"),
    SEARCH,
    function("F8", "Table", "Choose a character table (planned)"),
    FILES,
    QUIT,
    MODULES,
    RELOAD,
    STRINGS,
    DIRECTION,
    LEAVE,
    REPLACE,
    REREAD,
    function("Ctrl-F6", "Tab", "Toggle tab expansion"),
    OTHER,
    ARGUMENTS,
    SAVE_STATE,
    PREV_FILE,
    FOLLOWING_FILE,
    NEXT_SEARCH,
];

const TEXT_MORE: &[Entry] = &[
    CURSORS,
    key("Ctrl-Right", "Scroll columns to the right (planned)"),
    key("", "Shift the wrapped view right (planned)"),
    key("Ctrl-Left", "Scroll columns to the left (planned)"),
    key("", "Shift the wrapped view left (planned)"),
    FIRST_BYTE,
    LAST_BYTE,
    key("Ctrl-PgDn", "Scroll one screen row up (planned)"),
    key("Ctrl-PgUp", "Scroll one screen row down (planned)"),
    section(26, "Mouse"),
    key("Wheel click", "Return to the former position (planned)"),
];

const CODE: &[Entry] = &[
    key("Enter", "Switch to Text"),
    ESCAPE,
    FILE_HISTORY,
    NEXT_FILE,
    SCREEN,
    CALCULATOR,
    INFORMATION,
    key("Ctrl-Left", "Shorten the opcode column (planned)"),
    key("Ctrl-Right", "Widen the opcode column (planned)"),
    REPEAT,
    key("/", "Redecode from the current byte"),
    COMMENT,
    FUNCTIONS,
    WRITE_BLOCK,
    EDIT,
    MODE,
    GOTO,
    function(
        "F6",
        "Refer",
        "Find references to the current address (planned)",
    ),
    SEARCH,
    HEADER,
    FILES,
    QUIT,
    MODULES,
    NAMES,
    GLOBAL,
    FILL,
    RELOAD,
    function("Alt-F5", "Ord___", "Toggle ordinal display (planned)"),
    STRINGS,
    DIRECTION,
    TABLE,
    function(
        "Alt-F9",
        "Cmd/1b",
        "Switch the reference scan stride (planned)",
    ),
    LEAVE,
    function("Alt-F11", "Naked", "Toggle import-name labels (planned)"),
    ADD_NAME,
    BITNESS,
    IMPORT,
    REPLACE,
    REREAD,
    BASE,
    function("Ctrl-F6", "NextRf", "Find the next reference (planned)"),
    OTHER,
];

const CODE_MORE: &[Entry] = &[
    NEXT_SEARCH,
    CURSORS,
    FIRST_BYTE,
    LAST_BYTE,
    key("1..9", "Follow the numbered branch target"),
    key("Backspace", "Return from the branch target"),
    key("Up / Down", "Move by decoded instructions"),
    key("Left / Right", "Move by bytes"),
    key("PgUp / PgDn", "Move by decoded pages"),
    key("Shift-F1 / Ctrl-A", "Choose decoder, endian, or raw base"),
];

const VIEW_FALLBACKS: &[Entry] = &[
    section(20, "hiewbie keys"),
    key("Ctrl-E", "Start editing through the byte input"),
    key("Ctrl-G", "Open the address prompt"),
    key("Ctrl-F", "Open the pattern prompt"),
    key("Ctrl-R", "Repeat the last search"),
    key("Ctrl-O", "Inspect the executable"),
    key("Ctrl-C", "Exit this view"),
];

const DISCARD: Entry = key("Escape", "Discard the pending edits");
const RESTORE: Entry = function("F3", "Undo", "Restore this byte (Ctrl-Z/Y for undo/redo)");
const CRYPT: Entry = function("F7", "Crypt", "Transform the edit range (planned)");
const UPDATE: Entry = function("F9", "Update", "Commit pending bytes (also Ctrl-S)");
const TRUNCATE: Entry = function("F10", "Trunc", "Shorten the file here (planned)");
const CRYPT_SETTINGS: Entry = function(
    "Ctrl-F7",
    "CrySet",
    "Configure the transformation (planned)",
);
const RESTORE_PREVIOUS: Entry = key("Backspace", "Restore prior byte or cancel a pending digit");

const HEX_EDIT: &[Entry] = &[
    key("Tab", "Switch between Hex and ASCII input"),
    DISCARD,
    FUNCTIONS,
    RESTORE,
    function("F4", "Byte", "Open byte bit editing (planned)"),
    function("F5", "Word", "Open word bit editing (planned)"),
    function("F6", "Dword", "Open doubleword bit editing (planned)"),
    CRYPT,
    UPDATE,
    TRUNCATE,
    CRYPT_SETTINGS,
    function("Alt-F3", "Dword", "Edit the doubleword here (planned)"),
    function("Alt-F4", "Qword", "Edit the quadword here (planned)"),
    TABLE,
    CURSORS,
    FIRST_BYTE,
    LAST_BYTE,
    RESTORE_PREVIOUS,
];

const CODE_EDIT: &[Entry] = &[
    DISCARD,
    key("Enter", "Open the assembler (planned)"),
    FUNCTIONS,
    function("F2", "Asm", "Open the assembler (planned)"),
    RESTORE,
    CRYPT,
    UPDATE,
    TRUNCATE,
    CRYPT_SETTINGS,
    function(
        "Alt-F2",
        "NOPs",
        "Replace the current instruction with NOPs (planned)",
    ),
    TABLE,
    CURSORS,
    FIRST_BYTE,
    LAST_BYTE,
    RESTORE_PREVIOUS,
];

const FILE_HISTORY_HELP: &[Entry] = &[
    function("F2", "Path", "Toggle the full file path"),
    function("F5", "Mark", "Mark a file for navigation"),
    function("F8", "Delete", "Remove the selected history entry"),
    function("F9", "Files", "Open the file browser"),
    function("Ctrl-F5", "MrkAll", "Mark all rows"),
    function("Ctrl-F8", "Clear", "Clear history (planned)"),
    function("Alt-F5", "UnMark", "Remove all marks"),
    key(
        "Ctrl-Enter",
        "Open the highlighted entry with alternate options (planned)",
    ),
];
const FILE_PICKER: &[Entry] = &[
    function("F2", "Hidden", "Show or hide hidden entries"),
    function("F3", "Name", "Sort by filename"),
    function("F4", "Exten", "Sort by file extension"),
    function("F5", "Time", "Sort by modification time"),
    function("F6", "Size", "Sort by number of bytes"),
    function("F7", "Unsort", "Use filesystem listing order"),
    function("F8", "Revers", "Reverse the active sort"),
    function("F9", "Files", "Show file history"),
    function("F10", "Filter", "Set a filename mask"),
    function("Alt-F1", "Drive", "Choose a drive (planned)"),
    function("Alt-F2", "Drive", "Choose a drive (planned)"),
    function("Alt-F3", "Device", "Choose a device (planned)"),
    function("Alt-F4", "ReRead", "Reload the current directory"),
    function("Ctrl-F1", "Store1", "Store the first path (planned)"),
    key("Ctrl-F2", "Recall the first path (planned)"),
    function("Ctrl-F3", "Store2", "Store the second path (planned)"),
    key("Ctrl-F4", "Recall the second path (planned)"),
    key("Enter", "Open the selected file or directory"),
    key("Escape", "Leave the file list"),
    key("Up / Down", "Move one row"),
    key("PgUp / PgDn", "Move by a page"),
    key("Home / End", "Select the first or last row"),
];

const FILE_FILTER: &[Entry] = &[
    key("Enter", "Apply the filename mask"),
    key("Escape", "Cancel the mask change"),
    key("PgUp", "Clear the field"),
    key("PgDn", "Recall mask history (planned)"),
    key("Insert", "Toggle overwrite or insert"),
    key("Ctrl-End, Alt-X", "Erase from the caret to the end"),
    key("Shift-Insert", "Paste from the clipboard (planned)"),
    section(14, "Wildcard symbols"),
    key("?", "Match one character"),
    key("*", "Match any number of characters"),
    key("{ABC}", "Match one member of a set (planned)"),
    key("{A-C}", "Match one member of a range (planned)"),
    key("{!ABC}", "Match outside a set (planned)"),
    key("!", "Negate a mask (planned)"),
];

const FIND: &[Entry] = &[
    function("F2", "Direct", "Reverse the search direction"),
    function("F4", "Area", "Switch between the file and a marked range"),
    function("F5", "GoLast", "Jump to the most recent hit"),
    function("F6", "Even/Odd", "Select Unicode alignment (planned)"),
    function("F7", "Asm", "Enter an instruction pattern (planned)"),
    function(
        "Ctrl-F1",
        "OpSize",
        "Change the pattern instruction width (planned)",
    ),
    key("Alt-?", "Insert a wildcard byte (planned)"),
    key("Tab, Up, Down", "Switch between ASCII and Hex"),
    key("Enter", "Search. UTF-8 folds ASCII letters only"),
    key("Escape", "Close the search prompt"),
    key("Ctrl-U", "Clear the active field"),
    key("PgDn", "Browse recent patterns (planned)"),
    key("Insert", "Toggle insertion in the field (planned)"),
    key("Ctrl-End, Alt-X", "Trim the rest of the field (planned)"),
    key("Shift-Insert", "Clipboard paste is unavailable"),
];

const CALCULATOR_HELP: &[Entry] = &[
    Entry::Text("64-bit integer expressions with grouping by parentheses."),
    Entry::Text(""),
    Entry::Text("+   Add                 ~   Bitwise complement"),
    Entry::Text("-   Subtract            &   Bitwise AND"),
    Entry::Text("*   Multiply            ^   Bitwise XOR"),
    Entry::Text("/   Divide (signed)     |   Bitwise OR"),
    Entry::Text("%   Remainder"),
    Entry::Text(""),
    Entry::Text("!   Logical NOT         <   Less than"),
    Entry::Text("&&  Logical AND         >   Greater than"),
    Entry::Text("||  Logical OR          <=  At most"),
    Entry::Text("                        >=  At least"),
    Entry::Text("<<  Shift left          ==  Equal"),
    Entry::Text(">>  Shift right         !=  Not equal"),
    Entry::Text(""),
    Entry::Text("Bare integers use hexadecimal notation."),
    Entry::Text("An expression can mix bases with these prefixes and suffixes."),
    Entry::Text(""),
    Entry::Text("0x10 or 10h             Hexadecimal"),
    Entry::Text("10i / 10o / 10t         Binary / octal / decimal"),
    Entry::Text("\"AB\"                    Eight ASCII bytes at most, high byte first"),
    Entry::Text(""),
    Entry::Text("@b @w @d @q             Signed reads of 1, 2, 4, or 8 bytes"),
    Entry::Text("@B @W @D @Q             Unsigned reads in the same widths"),
    Entry::Text("@o / @O                 Local address / file offset"),
    Entry::Text(""),
    Entry::Text("Enter evaluates. Insert toggles replacement and insertion."),
    Entry::Text("Page Up clears the input. Escape returns to the editor."),
];
