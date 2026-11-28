//! Display parsed names without treating unresolved imports as executable targets.

use hiewbie_core::symbols::SymbolTarget;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use super::analysis::{AnalysisDialog, AnalysisPage};
use super::render::{HEADER, popup};
use super::{ByteSource, Ui, escaped_text};

impl<S: ByteSource> Ui<S> {
    pub(super) fn symbol_index(&self, page: AnalysisPage, selected: usize) -> Option<usize> {
        self.editor
            .symbols()?
            .entries
            .iter()
            .enumerate()
            .filter(|(_, symbol)| page.symbol_kind().is_none_or(|kind| symbol.kind == kind))
            .nth(selected)
            .map(|(index, _)| index)
    }

    pub(super) fn render_symbol_dialog(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        dialog: &AnalysisDialog,
    ) {
        let popup = popup(area, 110, 23);
        let title = match dialog.page {
            AnalysisPage::Imports => " Parsed imports ",
            AnalysisPage::Exports => " Parsed exports ",
            _ => " Parsed symbols ",
        };
        let mut lines = vec![
            Line::raw("F7 imports / F9 exports / N symbols / F8 headers"),
            Line::raw("Enter target / R metadata record / Esc close"),
        ];
        if let Some(table) = self.editor.symbols() {
            let count = table
                .entries
                .iter()
                .filter(|symbol| {
                    dialog
                        .page
                        .symbol_kind()
                        .is_none_or(|kind| symbol.kind == kind)
                })
                .count();
            lines.push(Line::raw(format!(
                "{count} parsed entries | {} coverage notes | generation {}",
                table.notes.len(),
                table.generation
            )));
            // Keep the selected name visible in a short terminal. Lower-priority
            // details/coverage lines may clip, but never the whole selectable list.
            let visible = popup.height.saturating_sub(9).max(1) as usize;
            let start = dialog.selected.saturating_sub(visible.saturating_sub(1));
            for (position, (index, symbol)) in table
                .entries
                .iter()
                .enumerate()
                .filter(|(_, symbol)| {
                    dialog
                        .page
                        .symbol_kind()
                        .is_none_or(|kind| symbol.kind == kind)
                })
                .enumerate()
                .skip(start)
                .take(visible)
            {
                let target = match symbol.target {
                    SymbolTarget::Virtual(address) => format!("VA {:X}", address.get()),
                    SymbolTarget::Arm(address) => format!("ARM VA {:X}", address.get()),
                    SymbolTarget::Thumb(address) => format!("Thumb VA {:X}", address.get()),
                    SymbolTarget::File(offset) => format!("FO {:X}", offset.get()),
                    SymbolTarget::ImportSlot(address) => format!("slot VA {:X}", address.get()),
                    SymbolTarget::Unavailable => "no target".to_owned(),
                };
                let library = symbol
                    .library
                    .as_ref()
                    .map_or_else(String::new, |library| format!("{}!", escaped_text(library)));
                let text = format!(
                    "{index:>4} {} {target} {library}{}",
                    symbol.kind,
                    escaped_text(&symbol.name)
                );
                let style = if position == dialog.selected {
                    Style::new().fg(Color::Black).bg(Color::Yellow)
                } else {
                    HEADER
                };
                lines.push(Line::styled(text, style));
            }
            if count == 0 {
                lines.push(Line::raw("<no entries in the parsed tables>"));
            }
            if let Some(index) = self.symbol_index(dialog.page, dialog.selected) {
                let symbol = &table.entries[index];
                lines.push(Line::raw(format!(
                    "Record FO {:X}+{:X}; {}",
                    symbol.record.offset.get(),
                    symbol.record.len,
                    escaped_text(&symbol.detail)
                )));
            }
            if let Some(note) = table.notes.first() {
                lines.push(Line::raw(format!("Coverage: {}", escaped_text(note))));
                if table.notes.len() > 1 {
                    lines.push(Line::raw("Use --symbols to print every coverage note."));
                }
            }
        } else {
            lines.push(Line::raw(
                "<symbol inspection unavailable; raw/header/Code access remains>",
            ));
            if let Some(problem) = self.editor.symbol_problem() {
                lines.push(Line::raw(escaped_text(problem)));
            }
        }
        frame.render_widget(Clear, popup);
        frame.render_widget(
            Paragraph::new(lines)
                .style(HEADER)
                .block(Block::default().borders(Borders::ALL).title(title)),
            popup,
        );
    }
}
