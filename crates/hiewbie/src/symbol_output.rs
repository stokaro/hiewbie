//! Bounded, escaped symbol output for scripts and non-terminal inspection.

use std::io::{self, Write};

use hiewbie_core::symbols::{SymbolKind, SymbolTarget};
use hiewbie_core::{ByteSource, Editor};

use crate::terminal::escaped_text;

const MAX_OUTPUT_SYMBOLS: usize = 64;

pub(crate) fn write<S: ByteSource>(
    editor: &mut Editor<S>,
    kind: Option<SymbolKind>,
    output: &mut impl Write,
) -> io::Result<()> {
    editor.refresh_symbols();
    let table = editor.symbols().ok_or_else(|| {
        io::Error::other(
            editor
                .symbol_problem()
                .unwrap_or("symbol tables are unavailable"),
        )
    })?;
    let image = editor
        .executable()
        .ok_or_else(|| io::Error::other("executable metadata is unavailable"))?;
    writeln!(
        output,
        "Format: {}; machine {}; generation {}",
        image.format, image.machine, table.generation
    )?;
    writeln!(
        output,
        "Image: FO {:016X}+{:X}",
        table.slice.offset.get(),
        table.slice.len
    )?;
    if let Some(index) = image.selected_slice {
        writeln!(
            output,
            "Selected slice: {index} of {} (zero-based)",
            image.slices.len()
        )?;
    }
    let count = table
        .entries
        .iter()
        .filter(|symbol| kind.is_none_or(|kind| symbol.kind == kind))
        .count();
    let category = match kind {
        Some(SymbolKind::Import) => "imports",
        Some(SymbolKind::Export) => "exports",
        _ => "symbols",
    };
    writeln!(output, "Parsed {category} ({count}):")?;
    for (index, symbol) in table
        .entries
        .iter()
        .enumerate()
        .filter(|(_, symbol)| kind.is_none_or(|kind| symbol.kind == kind))
        .take(MAX_OUTPUT_SYMBOLS)
    {
        writeln!(
            output,
            "  [{index}] {} {}",
            symbol.kind,
            escaped_text(&symbol.name)
        )?;
        if let Some(library) = &symbol.library {
            writeln!(output, "    Library: {}", escaped_text(library))?;
        }
        if let Some(ordinal) = symbol.ordinal {
            writeln!(output, "    Ordinal: {ordinal}")?;
        }
        if let Some(size) = symbol.size {
            writeln!(output, "    Declared size: {size}")?;
        }
        write!(output, "    Target: ")?;
        match symbol.target {
            SymbolTarget::Virtual(address) => write!(output, "VA {:016X}", address.get())?,
            SymbolTarget::Arm(address) => write!(output, "ARM VA {:016X}", address.get())?,
            SymbolTarget::Thumb(address) => write!(output, "Thumb VA {:016X}", address.get())?,
            SymbolTarget::File(offset) => write!(output, "FO {:016X}", offset.get())?,
            SymbolTarget::ImportSlot(address) => write!(
                output,
                "pointer slot VA {:016X} (not resolved external code)",
                address.get()
            )?,
            SymbolTarget::Unavailable => write!(output, "unavailable")?,
        }
        match symbol.target.file_offset(image) {
            Ok(offset) => writeln!(output, "; file FO {:016X}", offset.get())?,
            Err(_) => writeln!(output, "; no unambiguous file-backed target")?,
        }
        writeln!(
            output,
            "    Record: FO {:016X}+{:X}; {}",
            symbol.record.offset.get(),
            symbol.record.len,
            escaped_text(&symbol.detail)
        )?;
    }
    if count > MAX_OUTPUT_SYMBOLS {
        writeln!(
            output,
            "  ... {} additional {category} omitted",
            count - MAX_OUTPUT_SYMBOLS
        )?;
    }
    if count == 0 {
        writeln!(output, "  <no entries in the parsed tables>")?;
    }
    writeln!(output, "Coverage notes ({}):", table.notes.len())?;
    for note in &table.notes {
        writeln!(output, "  {}", escaped_text(note))?;
    }
    writeln!(
        output,
        "Scope: these parsed tables do not establish a complete loader import/export inventory. Records expose metadata bytes; no structural repair is performed."
    )
}
