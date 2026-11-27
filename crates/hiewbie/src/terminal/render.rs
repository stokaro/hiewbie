use hiewbie_core::{ByteSource, FileOffset, MAX_READ_BYTES, ViewMode};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use super::{Dialog, EntryColumn, InputDialog, PromptKind, Ui, escaped_text, printable};

const BASE: Style = Style::new().fg(Color::Gray).bg(Color::Blue);
const ADDRESS: Style = Style::new().fg(Color::Cyan).bg(Color::Blue);
const SELECTED: Style = Style::new().fg(Color::Black).bg(Color::Cyan);
const ACTIVE: Style = Style::new().fg(Color::Black).bg(Color::Yellow);
const HEADER: Style = Style::new().fg(Color::Black).bg(Color::Cyan);

impl<S: ByteSource> Ui<S> {
    pub(super) fn render(&mut self, frame: &mut Frame<'_>) {
        let area = frame.area();
        if area.width == 0 || area.height == 0 {
            return;
        }
        frame.render_widget(Block::default().style(BASE), area);
        let header = format!(" hiewbie {} | {}", env!("CARGO_PKG_VERSION"), self.label);
        frame.render_widget(Paragraph::new(header).style(HEADER), row(area, 0));
        self.update_viewport(area);

        if area.width < 23 || area.height < 5 {
            if area.height > 1 {
                frame.render_widget(
                    Paragraph::new("Resize: at least 23 x 5").style(BASE),
                    row(area, 1),
                );
            }
            return;
        }

        let mode = match self.editor.mode() {
            ViewMode::Hex => "HEX",
            ViewMode::Text => "TEXT",
        };
        let state = if self.reopen_required {
            "REOPEN"
        } else if self.editor.is_editing() {
            "EDIT"
        } else {
            "VIEW"
        };
        let capability = if self.editor.is_read_only() {
            "RO"
        } else {
            "RW"
        };
        let dirty = if self.editor.document().is_dirty() {
            "DIRTY"
        } else {
            "CLEAN"
        };
        let entry = if self.editor.is_editing() {
            match self.entry_column {
                EntryColumn::Hex => " HEX ENTRY",
                EntryColumn::Ascii => " ASCII ENTRY",
            }
        } else {
            ""
        };
        let status = format!(
            "{:016X} / {:016X}  {mode} {state} {capability} {dirty}{entry}",
            self.editor.cursor().get(),
            self.editor.document().len()
        );
        frame.render_widget(Paragraph::new(status).style(ADDRESS), row(area, 1));

        self.render_bytes(frame, area);
        frame.render_widget(
            Paragraph::new(self.message.as_str()).style(BASE.fg(Color::Yellow)),
            row(area, area.height - 2),
        );
        self.render_function_bar(frame, row(area, area.height - 1));
        match self.dialog.as_ref() {
            Some(Dialog::Help) => render_help(frame, area),
            Some(Dialog::Input(input)) => render_prompt(frame, area, input),
            None => {}
        }
    }

    fn update_viewport(&mut self, area: Rect) {
        self.bytes_per_row = match self.editor.mode() {
            ViewMode::Text => (area.width as usize).saturating_sub(18).clamp(1, 256),
            ViewMode::Hex => [64, 32, 16, 8, 4, 2, 1]
                .into_iter()
                .find(|count| 20 + 3 * count + (count - 1) / 2 <= area.width as usize)
                .unwrap_or(1),
        };
        self.visible_rows = (area.height.saturating_sub(4) as usize)
            .min(MAX_READ_BYTES / self.bytes_per_row)
            .max(1);
        let per_row = self.bytes_per_row as u64;
        self.top = self.top / per_row * per_row;
        let cursor = self.editor.cursor().get();
        if cursor < self.top {
            self.top = cursor / per_row * per_row;
        } else if cursor.saturating_sub(self.top) >= (self.visible_rows * self.bytes_per_row) as u64
        {
            let cursor_row = cursor / per_row;
            self.top = cursor_row.saturating_sub(self.visible_rows as u64 - 1) * per_row;
        }
    }

    fn render_bytes(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let body = Rect::new(area.x, area.y + 2, area.width, area.height - 4);
        if self.reopen_required {
            frame.render_widget(
                Paragraph::new("<file was replaced; exit and reopen to verify its saved state>")
                    .style(BASE),
                body,
            );
            return;
        }
        if self.editor.document().is_empty() {
            frame.render_widget(
                Paragraph::new("<empty file - overwrite requires existing bytes>").style(BASE),
                body,
            );
            return;
        }
        let remaining = self.editor.document().len().saturating_sub(self.top);
        let wanted = (self.visible_rows * self.bytes_per_row).min(MAX_READ_BYTES);
        let count = remaining.min(wanted as u64) as usize;
        let mut bytes = vec![0; count];
        let count = match self
            .editor
            .document_mut()
            .read_at(FileOffset::new(self.top), &mut bytes)
        {
            Ok(count) => count,
            Err(error) => {
                self.notice(format!("Read failed: {error}"));
                frame.render_widget(Paragraph::new("<bytes unavailable>").style(BASE), body);
                return;
            }
        };
        let lines = bytes[..count]
            .chunks(self.bytes_per_row)
            .enumerate()
            .map(|(index, bytes)| {
                let offset = self.top.saturating_add((index * self.bytes_per_row) as u64);
                self.byte_line(offset, bytes)
            })
            .collect::<Vec<_>>();
        frame.render_widget(Paragraph::new(lines).style(BASE), body);
    }

    fn byte_line(&self, offset: u64, bytes: &[u8]) -> Line<'static> {
        let mut spans = vec![Span::styled(format!("{offset:016X}  "), ADDRESS)];
        if self.editor.mode() == ViewMode::Hex {
            for index in 0..self.bytes_per_row {
                if index > 0 && index % 2 == 0 {
                    spans.push(Span::raw(" "));
                }
                let current = offset.checked_add(index as u64) == Some(self.editor.cursor().get());
                let text = match bytes.get(index) {
                    Some(_) if current && self.pending_nibble.is_some() => {
                        format!("{:X}_", self.pending_nibble.unwrap_or(0))
                    }
                    Some(byte) => format!("{byte:02X}"),
                    None => "  ".into(),
                };
                let style = if current && index < bytes.len() {
                    if self.editor.is_editing() && self.entry_column == EntryColumn::Hex {
                        ACTIVE
                    } else {
                        SELECTED
                    }
                } else {
                    BASE
                };
                spans.push(Span::styled(text, style));
            }
            spans.push(Span::raw("  "));
        }
        for (index, byte) in bytes.iter().enumerate() {
            let current = offset.checked_add(index as u64) == Some(self.editor.cursor().get());
            let style = if current {
                if self.editor.is_editing() && self.entry_column == EntryColumn::Ascii {
                    ACTIVE
                } else {
                    SELECTED
                }
            } else {
                BASE
            };
            spans.push(Span::styled(printable(*byte).to_string(), style));
        }
        Line::from(spans)
    }

    fn render_function_bar(&self, frame: &mut Frame<'_>, area: Rect) {
        let contextual = if self.reopen_required {
            Some(" File replaced | Esc exits | Reopen before further edits ")
        } else if self.search.is_some() {
            Some(" Searching in bounded steps | Esc cancel ")
        } else {
            match self.dialog.as_ref() {
                Some(Dialog::Help) => Some(" F1 / Enter / Esc close help "),
                Some(Dialog::Input(input)) if input.kind != PromptKind::Goto => {
                    Some(" Enter search | Tab hex/text | Ctrl+U clear | Esc cancel ")
                }
                Some(Dialog::Input(_)) => Some(" Enter goto | Ctrl+U clear | Esc cancel "),
                None => None,
            }
        };
        if let Some(text) = contextual {
            frame.render_widget(Paragraph::new(text).style(HEADER), area);
            return;
        }
        let labels = if self.editor.is_editing() {
            [
                "Help", "Asm-", "Rest-", "--", "--", "--", "--", "--", "Save", "Trunc-",
            ]
        } else {
            [
                "Help", "--", "Edit", "Mode", "Goto", "Refs-", "Search", "Hdr-", "Open-", "--",
            ]
        };
        let mut spans = Vec::with_capacity(20);
        for (index, label) in labels.into_iter().enumerate() {
            spans.push(Span::styled(
                format!("{:>2}", index + 1),
                BASE.fg(Color::Yellow).add_modifier(Modifier::BOLD),
            ));
            let available = !label.contains('-');
            let style = if available {
                HEADER
            } else {
                HEADER.fg(Color::DarkGray)
            };
            spans.push(Span::styled(format!("{label:<6}"), style));
        }
        frame.render_widget(Paragraph::new(Line::from(spans)).style(HEADER), area);
    }
}

fn row(area: Rect, relative_y: u16) -> Rect {
    Rect::new(area.x, area.y.saturating_add(relative_y), area.width, 1)
}

fn popup(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    )
}

fn render_help(frame: &mut Frame<'_>, area: Rect) {
    let popup = popup(area, 78, 23);
    let lines = [
        "Raw-byte Hex and fixed-width ASCII Text views share one cursor.",
        "",
        "View  Enter/F4 Hex/Text   F3/Ctrl+E edit   Esc quit",
        "      F5/Ctrl+G goto     F7/Ctrl+F literal search",
        "      Ctrl+Enter/Ctrl+R repeat search (forward, no wrap)",
        "",
        "Edit  F9/Ctrl+S save     Esc discard the transaction",
        "      Tab hex/ASCII     Ctrl+Z undo      Ctrl+Y redo",
        "      Two digits commit one hex byte; Backspace cancels a digit",
        "",
        "Move  Arrows bytes/rows  PgUp/PgDn pages",
        "      Home/End row edges  Ctrl+Home/End file edges",
        "",
        "Goto  Hex default, 0x optional, decimal suffix t, +/- relative",
        "Find  Tab changes hex/UTF-8 text; Esc cancels incremental scan",
        "Paste is rejected. Type printable ASCII; use hex for other bytes.",
        "",
        "Planned: Code, executable formats, assembly, blocks, multi-file.",
        "F9 in view means Open (planned), never Save. '-' means planned.",
        "Legacy terminals may intercept keys; use the Ctrl alternatives.",
        "Close help with F1, Enter, or Esc.",
    ]
    .into_iter()
    .map(Line::raw)
    .collect::<Vec<_>>();
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(lines).style(HEADER).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" hiewbie help ")
                .style(HEADER),
        ),
        popup,
    );
}

fn render_prompt(frame: &mut Frame<'_>, area: Rect, input: &InputDialog) {
    let popup = popup(area, 74, 8);
    let (title, hint) = match input.kind {
        PromptKind::Goto => (
            " Goto file offset ",
            "Hex default; decimal suffix t; +/- relative. Virtual addresses planned.",
        ),
        PromptKind::SearchHex => (
            " Search HEX (Tab: Text) ",
            "Pairs of hex digits, e.g. 48 65 6C 6C 6F. Forward search; no wrap.",
        ),
        PromptKind::SearchText => (
            " Search UTF-8 TEXT (Tab: Hex) ",
            "Literal UTF-8 bytes; case-sensitive. Forward search; no wrap.",
        ),
    };
    // Display the tail of long input; all input is retained for validation.
    let available = popup.width.saturating_sub(6) as usize;
    let escaped = escaped_text(&input.input);
    let tail = escaped
        .chars()
        .rev()
        .take(available)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();
    let lines = vec![
        Line::raw(hint),
        Line::raw(""),
        Line::styled(
            format!("> {tail}_"),
            Style::new().fg(Color::Yellow).bg(Color::Blue),
        ),
        Line::raw(
            input
                .error
                .as_deref()
                .unwrap_or("Enter applies; Ctrl+U clears; Esc cancels"),
        ),
    ];
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(lines)
            .style(HEADER)
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(title)
                    .style(HEADER),
            ),
        popup,
    );
}
