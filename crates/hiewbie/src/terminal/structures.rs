//! Navigate format metadata separately from its optional checked target bytes.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Wrap};

use super::analysis::AnalysisDialog;
use super::render::{PANEL, dialog_surface};
use super::{ByteSource, Ui, escaped_text};

impl<S: ByteSource> Ui<S> {
    pub(super) fn render_structure_dialog(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        dialog: &AnalysisDialog,
    ) {
        let content = dialog_surface(frame, area, 110, 26, " Format structure records ", PANEL);
        let mut lines = vec![
            Line::raw("Enter target / R record / F8 headers / Esc close"),
            Line::raw("Up/Down, PgUp/PgDn, Home/End select; Left/Right scroll detail"),
        ];
        let mut detail = String::new();
        if let Some(image) = self.editor.executable() {
            lines.push(Line::raw(format!(
                "{} records | generation {} | {} coverage notes",
                image.records.len(),
                image.generation,
                image.notes.len()
            )));
            // Prioritize a selectable row on small terminals; details can also
            // be printed completely through the same paged CLI record model.
            let visible = content.height.saturating_sub(9).clamp(1, 8) as usize;
            let start = dialog.selected.saturating_sub(visible.saturating_sub(1));
            for (index, record) in image.records.iter().enumerate().skip(start).take(visible) {
                let style = if index == dialog.selected {
                    Style::new().fg(Color::Black).bg(Color::Yellow)
                } else {
                    PANEL
                };
                lines.push(Line::styled(
                    format!(
                        "{index:>4} {} | {} | {}",
                        record.status,
                        record.kind,
                        escaped_text(&record.name)
                    ),
                    style,
                ));
            }
            if let Some(record) = image.records.get(dialog.selected) {
                lines.push(Line::raw(format!(
                    "Record FO {:X}+{:X}",
                    record.record.offset.get(),
                    record.record.len
                )));
                lines.push(Line::raw(
                    record.target.filter(|target| target.len > 0).map_or_else(
                        || "Target unavailable; R still follows metadata bytes".to_owned(),
                        |target| format!("Target FO {:X}+{:X}", target.offset.get(), target.len),
                    ),
                ));
                lines.push(Line::raw(format!(
                    "Detail at character {}; --structures --structure-page {} prints in full",
                    dialog.detail_scroll,
                    dialog.selected / crate::structure_output::PAGE_SIZE
                )));
                detail = escaped_text(&record.detail)
                    .chars()
                    .skip(dialog.detail_scroll)
                    .collect();
            } else {
                lines.push(Line::raw("<no structure records exposed for this image>"));
            }
        } else {
            lines.push(Line::raw(
                "<structures unavailable; raw bytes remain accessible>",
            ));
            if let Some(problem) = self.editor.analysis_problem() {
                detail = escaped_text(problem);
            }
        }
        let header_height = (lines.len() as u16).min(content.height);
        frame.render_widget(Paragraph::new(lines).style(PANEL), content);
        if content.height > header_height {
            let details = Rect {
                y: content.y + header_height,
                height: content.height - header_height,
                ..content
            };
            frame.render_widget(
                Paragraph::new(detail)
                    .style(PANEL)
                    .wrap(Wrap { trim: false }),
                details,
            );
        }
    }
}
