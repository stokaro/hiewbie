//! Complete, paged output of the same bounded records used by the terminal.

use std::io::{self, Write};

use hiewbie_core::{ByteSource, Editor};

use crate::terminal::escaped_text;

pub(crate) const PAGE_SIZE: usize = 64;

pub(crate) fn write<S: ByteSource>(
    editor: &mut Editor<S>,
    page: usize,
    output: &mut impl Write,
) -> io::Result<()> {
    editor.refresh_analysis();
    let image = editor.executable().ok_or_else(|| {
        io::Error::other(format!(
            "{}; use --dump for raw bytes",
            editor
                .analysis_problem()
                .unwrap_or("no executable metadata is available")
        ))
    })?;
    let start = page
        .checked_mul(PAGE_SIZE)
        .ok_or_else(|| io::Error::other("structure page index is too large"))?;
    let pages = image.records.len().div_ceil(PAGE_SIZE).max(1);
    if page >= pages {
        return Err(io::Error::other(format!(
            "structure page {page} is unavailable ({pages} pages)"
        )));
    }
    writeln!(
        output,
        "Format: {}; machine {}; generation {}",
        image.format, image.machine, image.generation
    )?;
    writeln!(
        output,
        "Image: FO {:016X}+{:X}; selected slice {:?}",
        image.slice.offset.get(),
        image.slice.len,
        image.selected_slice
    )?;
    writeln!(
        output,
        "Structure records ({}); page {page} of {pages} (zero-based, {PAGE_SIZE} per page):",
        image.records.len()
    )?;
    for (index, record) in image.records.iter().enumerate().skip(start).take(PAGE_SIZE) {
        writeln!(
            output,
            "  [{index}] {} | {} | {}",
            record.kind,
            record.status,
            escaped_text(&record.name)
        )?;
        writeln!(
            output,
            "    Record: FO {:016X}+{:X}",
            record.record.offset.get(),
            record.record.len
        )?;
        match record.target {
            Some(target) if target.len > 0 => writeln!(
                output,
                "    Target: FO {:016X}+{:X}",
                target.offset.get(),
                target.len
            )?,
            _ => writeln!(
                output,
                "    Target: unavailable (metadata record remains inspectable)"
            )?,
        }
        writeln!(output, "    Detail: {}", escaped_text(&record.detail))?;
    }
    if image.records.is_empty() {
        writeln!(output, "  <no structure records exposed for this image>")?;
    }
    if page + 1 < pages {
        writeln!(output, "Next: --structures --structure-page {}", page + 1)?;
    }
    writeln!(output, "Coverage notes ({}):", image.notes.len())?;
    for note in &image.notes {
        writeln!(output, "  {}", escaped_text(note))?;
    }
    writeln!(
        output,
        "Scope: metadata and checked file spans only; no relocation, repair, loading, or execution is performed."
    )
}
