use std::{cell::Cell, rc::Rc};

use hiewbie_core::{Document, FileOffset, formats, symbols};

use crate::{source::Source, word};

pub(crate) fn check(data: &[u8]) {
    // Both a physically present input and a virtual sparse file are inspected.
    // Fuzzed count fields therefore cannot hide behind an early small-file EOF.
    for len in [data.len() as u64, word(data, 0).max(data.len() as u64)] {
        let read = Rc::new(Cell::new(0));
        let limit = Rc::new(Cell::new(formats::MAX_FORMAT_READ_BYTES));
        let source = Source {
            bytes: data,
            len,
            short: 1 + usize::from(data.last().copied().unwrap_or(0)),
            read: read.clone(),
            limit: limit.clone(),
        };
        let mut document = Document::new(source);
        let parsed = formats::inspect(&mut document, None);
        assert!(read.get() <= formats::MAX_FORMAT_READ_BYTES);
        if let Ok(Some(image)) = parsed {
            assert!(image.regions.len() <= formats::MAX_FORMAT_ENTRIES);
            assert!(image.fields.len() <= formats::MAX_FORMAT_FIELDS);
            assert!(image.slices.len() <= formats::MAX_FAT_SLICES);
            assert_eq!(image.generation, 0);
            read.set(0);
            limit.set(symbols::MAX_SYMBOL_READ_BYTES);
            if let Ok(table) = symbols::inspect(&mut document, &image) {
                assert!(table.entries.len() <= symbols::MAX_SYMBOLS);
                assert!(table.notes.len() <= symbols::MAX_SYMBOL_NOTES);
                let text: usize = table
                    .entries
                    .iter()
                    .map(|symbol| {
                        symbol.name.len()
                            + symbol.library.as_ref().map_or(0, String::len)
                            + symbol.detail.len()
                    })
                    .chain(table.notes.iter().map(String::len))
                    .sum();
                assert!(text <= symbols::MAX_SYMBOL_TEXT_BYTES);
                for symbol in &table.entries {
                    // The parser bounds source bytes. Lossy UTF-8 decoding
                    // can expand each invalid source byte into U+FFFD (3 bytes).
                    assert!(symbol.name.len() <= 3 * symbols::MAX_SYMBOL_NAME_BYTES);
                    assert!(
                        symbol.library.as_ref().is_none_or(|name| {
                            name.len() <= 3 * symbols::MAX_SYMBOL_NAME_BYTES
                        })
                    );
                    if let Ok(offset) = symbol.target.file_offset(&image) {
                        assert!(image.slice.contains(offset));
                    }
                }
            }
            assert!(read.get() <= symbols::MAX_SYMBOL_READ_BYTES);
        }
        // Success, unknown magic and parse failure all preserve raw access.
        read.set(0);
        limit.set(data.len());
        let mut raw = vec![0; data.len()];
        assert_eq!(
            document.read_at(FileOffset::new(0), &mut raw).unwrap(),
            data.len()
        );
        assert_eq!(raw, data);
        assert_eq!(document.generation(), 0);
        assert!(!document.is_dirty());
        assert!(document.patches().is_empty());
        // Fuzz the optional slice selector separately; invalid selectors must
        // also leave raw bytes untouched and remain within the format budget.
        read.set(0);
        limit.set(formats::MAX_FORMAT_READ_BYTES);
        let _ = formats::inspect(&mut document, Some(word(data, 8) as usize));
        assert_eq!(document.generation(), 0);
        assert!(!document.is_dirty());
    }
}
