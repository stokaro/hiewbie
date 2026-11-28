//! Generated integer-field fixtures only; no executable samples are imported.

use super::*;
use crate::formats::Endian;
use crate::instructions::DecodeMode;
use crate::{Command, Document, Editor, FileOffset, MemorySource, formats};

fn put16(bytes: &mut [u8], at: usize, value: u16, endian: Endian) {
    bytes[at..at + 2].copy_from_slice(&match endian {
        Endian::Little => value.to_le_bytes(),
        Endian::Big => value.to_be_bytes(),
    });
}

fn put32(bytes: &mut [u8], at: usize, value: u32, endian: Endian) {
    bytes[at..at + 4].copy_from_slice(&match endian {
        Endian::Little => value.to_le_bytes(),
        Endian::Big => value.to_be_bytes(),
    });
}

fn put64(bytes: &mut [u8], at: usize, value: u64, endian: Endian) {
    bytes[at..at + 8].copy_from_slice(&match endian {
        Endian::Little => value.to_le_bytes(),
        Endian::Big => value.to_be_bytes(),
    });
}

fn aligned(value: usize, alignment: usize) -> usize {
    value.div_ceil(alignment) * alignment
}

struct Fixture {
    bytes: Vec<u8>,
    endian: Endian,
    width: usize,
    sections: Vec<usize>,
    addresses: Vec<u64>,
    symtab: usize,
    symbols: usize,
    strings: usize,
    dylibs: Vec<usize>,
    extra: Vec<usize>,
}

impl Fixture {
    fn value(&mut self, index: usize, value: u64) {
        if self.width == 16 {
            put64(
                &mut self.bytes,
                self.symbols + index * self.width + 8,
                value,
                self.endian,
            );
        } else {
            put32(
                &mut self.bytes,
                self.symbols + index * self.width + 8,
                value as u32,
                self.endian,
            );
        }
    }
}

fn command(kind: u32, size: usize, endian: Endian) -> Vec<u8> {
    let mut bytes = vec![0; size];
    put32(&mut bytes, 0, kind, endian);
    put32(&mut bytes, 4, size as u32, endian);
    bytes
}

fn fixture(bits: u8, endian: Endian) -> Fixture {
    fixture_with(
        bits,
        endian,
        &[(LC_LOAD_DYLIB, "/usr/lib/libDemo.dylib".into())],
        &[],
    )
}

fn fixture_with(
    bits: u8,
    endian: Endian,
    libraries: &[(u32, String)],
    extra: &[Vec<u8>],
) -> Fixture {
    let wide = bits == 64;
    let (header, segment_size, section_size, width) = if wide {
        (32, 72, 80, 16)
    } else {
        (28, 56, 68, 12)
    };
    let section_kinds = [0, 0x11, 0x13, 0x12, 1];
    let segment = header;
    let symtab = segment + segment_size + section_size * section_kinds.len();
    let mut commands = vec![
        command(
            if wide { LC_SEGMENT_64 } else { LC_SEGMENT },
            symtab - segment,
            endian,
        ),
        command(LC_SYMTAB, 24, endian),
    ];
    let mut dylibs = Vec::new();
    let mut next = symtab + 24;
    for (kind, name) in libraries {
        let size = aligned(24 + name.len() + 1, if wide { 8 } else { 4 });
        let mut bytes = command(*kind, size, endian);
        put32(&mut bytes, 8, 24, endian);
        bytes[24..24 + name.len()].copy_from_slice(name.as_bytes());
        dylibs.push(next);
        next += size;
        commands.push(bytes);
    }
    let mut extra_positions = Vec::new();
    for bytes in extra {
        extra_positions.push(next);
        next += bytes.len();
        commands.push(bytes.clone());
    }
    let data = aligned(next, 256);
    let symbols = aligned(data + 3 * 64, 256);
    let names = [
        "_entry",
        "_private",
        "_local",
        "_puts",
        "_absolute",
        "_common",
        "_alias",
        "_tls",
        "_tlv",
        "_tls_zero",
        "_bss",
        "_debug",
        "",
        "_prebound",
        "_end",
        "_dynamic",
        "_self",
        "_unknown",
    ];
    let strings = symbols + names.len() * width;
    let mut text = vec![0];
    let mut indices = Vec::new();
    for name in names {
        if name.is_empty() {
            indices.push(0);
        } else {
            indices.push(text.len() as u32);
            text.extend_from_slice(name.as_bytes());
            text.push(0);
        }
    }
    let file_size = aligned(strings + text.len(), 256);
    let mut bytes = vec![0; file_size];
    let mut at = header;
    for command in &commands {
        bytes[at..at + command.len()].copy_from_slice(command);
        at += command.len();
    }
    put32(
        &mut bytes,
        0,
        if wide { 0xfeed_facf } else { 0xfeed_face },
        endian,
    );
    put32(&mut bytes, 4, if wide { 0x0100_000c } else { 12 }, endian);
    put32(&mut bytes, 12, 2, endian);
    put32(&mut bytes, 16, commands.len() as u32, endian);
    put32(&mut bytes, 20, (next - header) as u32, endian);
    put32(&mut bytes, 24, 0x80, endian);
    bytes[segment + 8..segment + 14].copy_from_slice(b"__TEXT");
    if wide {
        put64(&mut bytes, segment + 24, 0x1000, endian);
        put64(&mut bytes, segment + 32, file_size as u64 + 0x100, endian);
        put64(&mut bytes, segment + 48, file_size as u64, endian);
        put32(&mut bytes, segment + 56, 7, endian);
        put32(&mut bytes, segment + 60, 5, endian);
        put32(&mut bytes, segment + 64, section_kinds.len() as u32, endian);
    } else {
        put32(&mut bytes, segment + 24, 0x1000, endian);
        put32(&mut bytes, segment + 28, file_size as u32 + 0x100, endian);
        put32(&mut bytes, segment + 36, file_size as u32, endian);
        put32(&mut bytes, segment + 40, 7, endian);
        put32(&mut bytes, segment + 44, 5, endian);
        put32(&mut bytes, segment + 48, section_kinds.len() as u32, endian);
    }
    let mut sections = Vec::new();
    let mut addresses = Vec::new();
    for (index, kind) in section_kinds.into_iter().enumerate() {
        let at = segment + segment_size + index * section_size;
        let zero = matches!(kind, 1 | 0x12);
        let offset = if zero { 0 } else { data + index * 64 };
        let address = 0x1000
            + if zero {
                file_size + (index - 3) * 64
            } else {
                offset
            } as u64;
        bytes[at..at + 6].copy_from_slice(b"__test");
        bytes[at + 16..at + 22].copy_from_slice(b"__TEXT");
        if wide {
            put64(&mut bytes, at + 32, address, endian);
            put64(&mut bytes, at + 40, 32, endian);
            put32(&mut bytes, at + 48, offset as u32, endian);
            put32(&mut bytes, at + 64, kind, endian);
        } else {
            put32(&mut bytes, at + 32, address as u32, endian);
            put32(&mut bytes, at + 36, 32, endian);
            put32(&mut bytes, at + 40, offset as u32, endian);
            put32(&mut bytes, at + 56, kind, endian);
        }
        sections.push(at);
        addresses.push(address);
    }
    put32(&mut bytes, symtab + 8, symbols as u32, endian);
    put32(&mut bytes, symtab + 12, names.len() as u32, endian);
    put32(&mut bytes, symtab + 16, strings as u32, endian);
    put32(&mut bytes, symtab + 20, text.len() as u32, endian);
    bytes[strings..strings + text.len()].copy_from_slice(&text);
    let entries = [
        (N_SECT | N_EXT, 1, 0, addresses[0] + 4),
        (N_SECT | N_EXT | N_PEXT, 1, 0, addresses[0] + 8),
        (N_SECT, 1, 0, addresses[0] + 12),
        (N_UNDF | N_EXT, 0, 0x140, 0),
        (N_ABS | N_EXT, 0, 0, 0x1234),
        (N_UNDF | N_EXT, 0, 0xf00, 0x20),
        (N_INDR | N_EXT, 0, 0, u64::from(indices[0])),
        (N_SECT | N_EXT, 2, 0, addresses[1] + 4),
        (N_SECT | N_EXT, 3, 0, addresses[2] + 4),
        (N_SECT | N_EXT, 4, 0, addresses[3] + 4),
        (N_SECT | N_EXT, 5, 0, addresses[4] + 4),
        (0x24, 255, 0, u64::from(u32::MAX)),
        (N_SECT | N_EXT, 1, 0, addresses[0]),
        (N_PBUD | N_EXT, 0, 0xff00, 0xaaaa),
        (N_SECT | N_EXT, 1, 0, addresses[0] + 32),
        (N_UNDF | N_EXT, 0, 0xfe00, 0),
        (N_UNDF | N_EXT, 0, 0, 0),
        (0x4 | N_EXT, 0, 0, 0x9876),
    ];
    for (index, (flags, section, description, value)) in entries.into_iter().enumerate() {
        let at = symbols + index * width;
        put32(&mut bytes, at, indices[index], endian);
        bytes[at + 4] = flags;
        bytes[at + 5] = section;
        put16(&mut bytes, at + 6, description, endian);
        if wide {
            put64(&mut bytes, at + 8, value, endian);
        } else {
            put32(&mut bytes, at + 8, value as u32, endian);
        }
    }
    Fixture {
        bytes,
        endian,
        width,
        sections,
        addresses,
        symtab,
        symbols,
        strings,
        dylibs,
        extra: extra_positions,
    }
}

fn inspect(bytes: Vec<u8>) -> Result<(Executable, SymbolTable), FormatError> {
    let mut document = Document::new(MemorySource::new(bytes));
    let image = formats::inspect(&mut document, None)?.expect("generated Mach-O header");
    let symbols = crate::symbols::inspect(&mut document, &image)?;
    Ok((image, symbols))
}

fn named<'a>(table: &'a SymbolTable, name: &str) -> &'a Symbol {
    table
        .entries
        .iter()
        .find(|symbol| symbol.name == name)
        .expect("fixture symbol")
}

#[test]
fn nlist_widths_and_byte_orders_preserve_names_kinds_records_and_targets() {
    for bits in [32, 64] {
        for endian in [Endian::Little, Endian::Big] {
            let fixture = fixture(bits, endian);
            let at = fixture.symbols;
            let address = fixture.addresses[0] + 4;
            let (image, table) = inspect(fixture.bytes).unwrap();
            assert_eq!(table.entries.len(), 16);
            let export = named(&table, "_entry");
            assert_eq!(export.kind, SymbolKind::Export);
            assert_eq!(export.record.offset.get(), at as u64);
            assert_eq!(export.record.len, u64::from(bits / 8) + 8);
            assert_eq!(
                export.target,
                SymbolTarget::Virtual(VirtualAddress::new(address))
            );
            assert_eq!(
                export.target.file_offset(&image),
                Ok(FileOffset::new(address - 0x1000))
            );
            assert_eq!(named(&table, "_private").kind, SymbolKind::Defined);
            assert!(
                named(&table, "_private")
                    .detail
                    .contains("private external")
            );
            assert_eq!(named(&table, "_local").kind, SymbolKind::Defined);
            let import = named(&table, "_puts");
            assert_eq!(import.kind, SymbolKind::Import);
            assert_eq!(import.library.as_deref(), Some("/usr/lib/libDemo.dylib"));
            assert_eq!(import.ordinal, Some(1));
            assert!(import.detail.contains("weak reference"));
            assert_eq!(
                named(&table, "_prebound").library.as_deref(),
                Some("<main executable>")
            );
            assert_eq!(
                named(&table, "_dynamic").library.as_deref(),
                Some("<dynamic lookup>")
            );
            assert_eq!(named(&table, "_self").library.as_deref(), Some("<self>"));
            assert!(table.notes.iter().any(|note| note.contains("STABS")));
            assert!(table.notes.iter().any(|note| note.contains("Unnamed")));
        }
    }
}

#[test]
fn unresolved_values_never_become_navigation_addresses() {
    let (image, table) = inspect(fixture(64, Endian::Little).bytes).unwrap();
    for name in [
        "_puts",
        "_absolute",
        "_common",
        "_alias",
        "_tls",
        "_tlv",
        "_tls_zero",
        "_bss",
        "_prebound",
        "_end",
        "_unknown",
    ] {
        let symbol = named(&table, name);
        assert_eq!(symbol.target, SymbolTarget::Unavailable, "{name}");
        assert!(symbol.target.file_offset(&image).is_err(), "{name}");
    }
    let common = named(&table, "_common");
    assert_eq!(common.kind, SymbolKind::Defined);
    assert_eq!(common.size, Some(32));
    assert_eq!(common.library, None);
    assert_eq!(common.ordinal, None);
    assert!(common.detail.contains("alignment 2^15"));
    assert!(named(&table, "_alias").detail.contains("_entry"));
    assert_eq!(named(&table, "_absolute").kind, SymbolKind::Export);
}

#[test]
fn library_ordinals_include_all_dependency_commands_and_old_ordinal_254() {
    let kinds = [
        LC_LOAD_DYLIB,
        LC_LOAD_WEAK_DYLIB,
        LC_REEXPORT_DYLIB,
        LC_LAZY_LOAD_DYLIB,
        LC_LOAD_UPWARD_DYLIB,
    ];
    let libraries = (0..254)
        .map(|index| (kinds[index % kinds.len()], format!("lib{index}.dylib")))
        .collect::<Vec<_>>();
    let mut fixture = fixture_with(64, Endian::Little, &libraries, &[]);
    put16(
        &mut fixture.bytes,
        fixture.symbols + 3 * fixture.width + 6,
        0x0500,
        Endian::Little,
    );
    let (_, table) = inspect(fixture.bytes).unwrap();
    assert_eq!(
        named(&table, "_puts").library.as_deref(),
        Some("lib4.dylib")
    );
    assert_eq!(
        named(&table, "_dynamic").library.as_deref(),
        Some("lib253.dylib")
    );
}

#[test]
fn flat_namespace_does_not_interpret_descriptor_as_library_ordinal() {
    let mut fixture = fixture(32, Endian::Big);
    put32(&mut fixture.bytes, 24, 0, Endian::Big);
    put16(
        &mut fixture.bytes,
        fixture.symbols + 3 * fixture.width + 6,
        0xfa00,
        Endian::Big,
    );
    let (_, table) = inspect(fixture.bytes).unwrap();
    let import = named(&table, "_puts");
    assert_eq!(import.library, None);
    assert_eq!(import.ordinal, None);
    assert!(import.detail.contains("flat namespace"));
}

#[test]
fn thumb_mode_is_marked_only_for_aligned_arm_definitions() {
    for bits in [32, 64] {
        let mut fixture = fixture(bits, Endian::Little);
        put16(&mut fixture.bytes, fixture.symbols + 6, 0x8, Endian::Little);
        let address = fixture.addresses[0] + 4;
        let (_, table) = inspect(fixture.bytes).unwrap();
        let expected = if bits == 32 {
            SymbolTarget::Thumb(VirtualAddress::new(address))
        } else {
            SymbolTarget::Virtual(VirtualAddress::new(address))
        };
        assert_eq!(named(&table, "_entry").target, expected);
    }
    let mut fixture = fixture(32, Endian::Little);
    put16(&mut fixture.bytes, fixture.symbols + 6, 0x8, Endian::Little);
    fixture.value(0, fixture.addresses[0] + 5);
    let (_, table) = inspect(fixture.bytes).unwrap();
    assert_eq!(named(&table, "_entry").target, SymbolTarget::Unavailable);
}

#[test]
fn missing_nlist_preserves_explicit_modern_loader_coverage_gaps() {
    let endian = Endian::Little;
    let commands = [
        command(LC_DYLD_INFO_ONLY, 48, endian),
        command(LC_DYLD_EXPORTS_TRIE, 16, endian),
        command(LC_DYLD_CHAINED_FIXUPS, 16, endian),
        command(LC_DYSYMTAB, 80, endian),
    ];
    let mut fixture = fixture_with(64, endian, &[], &commands);
    // A UUID-shaped unknown-to-this-reader command replaces LC_SYMTAB.
    put32(&mut fixture.bytes, fixture.symtab, 0x1b, endian);
    let (_, table) = inspect(fixture.bytes).unwrap();
    assert!(table.entries.is_empty());
    for gap in [
        "No Mach-O LC_SYMTAB",
        "export trie",
        "bind/rebase",
        "chained fixups",
        "LC_DYSYMTAB",
    ] {
        assert!(table.notes.iter().any(|note| note.contains(gap)), "{gap}");
    }
}

#[test]
fn malformed_nlist_strings_counts_and_section_associations_fail() {
    for bits in [32, 64] {
        for mutation in 0..8 {
            let mut fixture = fixture(bits, Endian::Big);
            match mutation {
                0 => put32(&mut fixture.bytes, fixture.symbols, u32::MAX, Endian::Big),
                1 => put32(&mut fixture.bytes, fixture.symtab + 20, 3, Endian::Big),
                2 => put32(
                    &mut fixture.bytes,
                    fixture.symtab + 12,
                    super::super::MAX_SYMBOLS as u32 + 1,
                    Endian::Big,
                ),
                3 => fixture.bytes[fixture.symbols + 5] = 0,
                4 => fixture.bytes[fixture.symbols + 5] = 6,
                5 => fixture.bytes[fixture.symbols + 3 * fixture.width + 5] = 1,
                6 => fixture.value(0, fixture.addresses[0] + 33),
                7 => fixture.value(6, u64::from(u32::MAX)),
                _ => unreachable!(),
            }
            assert!(
                inspect(fixture.bytes).is_err(),
                "bits={bits}, mutation={mutation}"
            );
        }
    }
}

#[test]
fn malformed_section_extents_library_names_and_ordinals_fail() {
    for mutation in 0..6 {
        let mut fixture = fixture(64, Endian::Little);
        match mutation {
            0 => put64(
                &mut fixture.bytes,
                fixture.sections[0] + 32,
                u64::MAX,
                Endian::Little,
            ),
            1 => put64(
                &mut fixture.bytes,
                fixture.sections[0] + 40,
                u64::MAX,
                Endian::Little,
            ),
            2 => put32(
                &mut fixture.bytes,
                fixture.sections[0] + 48,
                1,
                Endian::Little,
            ),
            3 => put32(&mut fixture.bytes, fixture.dylibs[0] + 8, 8, Endian::Little),
            4 => {
                let at = fixture.dylibs[0];
                let size = u32_at(&fixture.bytes, at + 4, Endian::Little) as usize;
                fixture.bytes[at + 24..at + size].fill(b'x');
            }
            5 => put16(
                &mut fixture.bytes,
                fixture.symbols + 3 * fixture.width + 6,
                0x0200,
                Endian::Little,
            ),
            _ => unreachable!(),
        }
        // A section's file offset that disagrees with its VA remains visible as
        // an unresolved definition; it cannot silently point into the header.
        if mutation == 2 {
            let (_, table) = inspect(fixture.bytes).unwrap();
            assert_eq!(named(&table, "_entry").target, SymbolTarget::Unavailable);
        } else {
            assert!(inspect(fixture.bytes).is_err(), "mutation={mutation}");
        }
    }
}

#[test]
fn symbol_name_limit_is_enforced_before_retaining_a_large_string() {
    let mut fixture = fixture(64, Endian::Little);
    let offset = fixture.bytes.len();
    fixture.bytes.push(0);
    fixture.bytes.extend(std::iter::repeat_n(
        b'x',
        super::super::MAX_SYMBOL_NAME_BYTES + 1,
    ));
    fixture.bytes.push(0);
    put32(&mut fixture.bytes, fixture.symtab + 12, 1, Endian::Little);
    put32(
        &mut fixture.bytes,
        fixture.symtab + 16,
        offset as u32,
        Endian::Little,
    );
    put32(
        &mut fixture.bytes,
        fixture.symtab + 20,
        (super::super::MAX_SYMBOL_NAME_BYTES + 3) as u32,
        Endian::Little,
    );
    assert!(matches!(
        inspect(fixture.bytes),
        Err(FormatError::Limit {
            resource: "symbol name bytes",
            ..
        })
    ));
}

#[test]
fn duplicate_symtab_and_out_of_slice_linkedit_ranges_fail() {
    let endian = Endian::Little;
    let fixture = fixture_with(64, endian, &[], &[command(LC_SYMTAB, 24, endian)]);
    assert!(inspect(fixture.bytes).is_err());
    for kind in [LC_DYLD_EXPORTS_TRIE, LC_DYLD_CHAINED_FIXUPS] {
        let mut fixture = fixture_with(64, endian, &[], &[command(kind, 16, endian)]);
        put32(&mut fixture.bytes, fixture.extra[0] + 8, u32::MAX, endian);
        put32(&mut fixture.bytes, fixture.extra[0] + 12, 1, endian);
        assert!(inspect(fixture.bytes).is_err());
    }
}

fn fat_fixture(first: &Fixture, second: &Fixture) -> Vec<u8> {
    let mut bytes = vec![0; 0x4000 + second.bytes.len()];
    put32(&mut bytes, 0, 0xcafe_babe, Endian::Big);
    put32(&mut bytes, 4, 2, Endian::Big);
    for (index, (offset, fixture)) in [(0x1000, first), (0x4000, second)].into_iter().enumerate() {
        let at = 8 + index * 20;
        let cpu = u32_at(&fixture.bytes, 4, fixture.endian);
        put32(&mut bytes, at, cpu, Endian::Big);
        put32(&mut bytes, at + 8, offset as u32, Endian::Big);
        put32(&mut bytes, at + 12, fixture.bytes.len() as u32, Endian::Big);
        put32(&mut bytes, at + 16, 12, Endian::Big);
        bytes[offset..offset + fixture.bytes.len()].copy_from_slice(&fixture.bytes);
    }
    bytes
}

#[test]
fn selected_fat_slice_controls_byte_order_and_global_record_offsets() {
    let first = fixture(64, Endian::Little);
    let second = fixture(32, Endian::Big);
    let second_record = second.symbols as u64;
    let second_target = second.addresses[0] + 4 - 0x1000;
    let mut document = Document::new(MemorySource::new(fat_fixture(&first, &second)));
    let image = formats::inspect(&mut document, Some(1)).unwrap().unwrap();
    let table = crate::symbols::inspect(&mut document, &image).unwrap();
    assert_eq!(image.bitness, 32);
    assert_eq!(image.endian, Endian::Big);
    let symbol = named(&table, "_entry");
    assert_eq!(symbol.record.offset.get(), 0x4000 + second_record);
    assert_eq!(
        symbol.target.file_offset(&image),
        Ok(FileOffset::new(0x4000 + second_target))
    );
    assert_eq!(table.slice, image.slice);
}

#[test]
fn editor_slice_selection_invalidates_symbols_without_changing_document_generation() {
    let mut first = fixture(32, Endian::Little);
    let mut second = fixture(64, Endian::Big);
    first.bytes[first.strings + 1] = b'A';
    second.bytes[second.strings + 1] = b'B';
    put16(&mut first.bytes, first.symbols + 6, 0x8, first.endian);
    let mut editor = Editor::new(
        Document::new(MemorySource::new(fat_fixture(&first, &second))),
        false,
    );
    let generation = editor.document().generation();
    editor.refresh_symbols();
    let table = editor.symbols().unwrap();
    assert_eq!(table.entries.len(), 16);
    assert_eq!(table.slice.offset.get(), 0x1000);
    assert_eq!(table.generation, generation);
    assert_eq!(
        named(table, "Aentry").record.offset.get(),
        0x1000 + first.symbols as u64
    );
    assert!(
        table
            .entries
            .iter()
            .all(|symbol| table.slice.contains(symbol.record.offset))
    );
    assert!(!table.entries.iter().any(|symbol| symbol.name == "Bentry"));
    let thumb_index = table
        .entries
        .iter()
        .position(|symbol| symbol.name == "Aentry")
        .unwrap();
    editor.dispatch(Command::GotoSymbol(thumb_index)).unwrap();
    assert_eq!(editor.decode_context().mode, Some(DecodeMode::Thumb));

    editor.dispatch(Command::SelectSlice(1)).unwrap();
    assert_eq!(editor.document().generation(), generation);
    assert!(editor.symbols().is_none());
    assert_eq!(editor.executable().unwrap().selected_slice, Some(1));
    assert_eq!(editor.decode_context().mode, Some(DecodeMode::Aarch64));
    editor.refresh_symbols();
    let table = editor.symbols().unwrap();
    assert_eq!(table.entries.len(), 16);
    assert_eq!(table.slice.offset.get(), 0x4000);
    assert_eq!(table.generation, generation);
    assert_eq!(
        named(table, "Bentry").record.offset.get(),
        0x4000 + second.symbols as u64
    );
    assert!(
        table
            .entries
            .iter()
            .all(|symbol| table.slice.contains(symbol.record.offset))
    );
    assert!(!table.entries.iter().any(|symbol| symbol.name == "Aentry"));

    editor
        .dispatch(Command::SetDecodeMode(Some(DecodeMode::X86_64)))
        .unwrap();
    editor.dispatch(Command::SelectSlice(0)).unwrap();
    assert_eq!(editor.document().generation(), generation);
    assert!(editor.symbols().is_none());
    assert_eq!(editor.decode_context().mode, Some(DecodeMode::X86_64));
}

#[test]
fn stripped_and_truncated_symbol_tables_keep_raw_bytes_unchanged() {
    let mut stripped = fixture(64, Endian::Little);
    put32(&mut stripped.bytes, stripped.symtab + 12, 0, Endian::Little);
    let (_, table) = inspect(stripped.bytes).unwrap();
    assert!(table.entries.is_empty());
    assert!(table.notes.iter().any(|note| note.contains("nlist")));

    let mut fixture = fixture(64, Endian::Little);
    let length = fixture.bytes.len();
    put32(
        &mut fixture.bytes,
        fixture.symtab + 8,
        length as u32 - 1,
        Endian::Little,
    );
    let mut document = Document::new(MemorySource::new(fixture.bytes.clone()));
    let image = formats::inspect(&mut document, None).unwrap().unwrap();
    assert!(crate::symbols::inspect(&mut document, &image).is_err());
    assert_eq!(document.source().as_slice(), fixture.bytes);
    assert_eq!(document.generation(), 0);
}

#[test]
fn relocatable_symbols_use_section_bytes_when_image_mapping_differs() {
    let mut fixture = fixture(64, Endian::Little);
    put32(&mut fixture.bytes, 12, 1, Endian::Little);
    let section_file = u32_at(&fixture.bytes, fixture.sections[0] + 48, Endian::Little);
    put32(
        &mut fixture.bytes,
        fixture.sections[0] + 48,
        section_file + 4,
        Endian::Little,
    );
    let (_, table) = inspect(fixture.bytes).unwrap();
    assert_eq!(
        named(&table, "_entry").target,
        SymbolTarget::File(FileOffset::new(u64::from(section_file) + 8))
    );
}

#[test]
fn section_zero_fill_and_resolvers_never_borrow_unrelated_mapped_bytes() {
    let mut fixture = fixture(64, Endian::Little);
    put64(
        &mut fixture.bytes,
        fixture.sections[4] + 32,
        fixture.addresses[0],
        Endian::Little,
    );
    fixture.value(10, fixture.addresses[0] + 4);
    put32(&mut fixture.bytes, 12, 1, Endian::Little);
    put16(
        &mut fixture.bytes,
        fixture.symbols + 6,
        0x100,
        Endian::Little,
    );
    let (image, table) = inspect(fixture.bytes).unwrap();
    assert!(
        image
            .va_to_file(VirtualAddress::new(fixture.addresses[0] + 4))
            .is_ok()
    );
    assert_eq!(named(&table, "_bss").target, SymbolTarget::Unavailable);
    assert_eq!(named(&table, "_entry").target, SymbolTarget::Unavailable);
}

#[test]
fn invalid_utf8_names_are_lossy_text_without_changing_underlying_bytes() {
    let mut fixture = fixture(32, Endian::Big);
    fixture.bytes[fixture.strings + 1] = 0xff;
    let original = fixture.bytes.clone();
    let mut document = Document::new(MemorySource::new(fixture.bytes));
    let image = formats::inspect(&mut document, None).unwrap().unwrap();
    let table = crate::symbols::inspect(&mut document, &image).unwrap();
    assert_eq!(table.entries[0].name, "\u{fffd}entry");
    assert_eq!(document.source().as_slice(), original);
}

#[test]
fn fat64_global_symbol_offsets_can_exceed_four_gibibytes() {
    struct Sparse {
        prefix: Vec<u8>,
        member: Vec<u8>,
        member_offset: u64,
        bytes_read: usize,
    }
    impl ByteSource for Sparse {
        fn len(&self) -> u64 {
            self.member_offset + self.member.len() as u64
        }

        fn read_at(&mut self, offset: FileOffset, out: &mut [u8]) -> std::io::Result<usize> {
            let start = offset.get();
            let length = self.len().saturating_sub(start).min(out.len() as u64) as usize;
            out[..length].fill(0);
            for (base, bytes) in [(0, &self.prefix), (self.member_offset, &self.member)] {
                let low = start.max(base);
                let high = (start + length as u64).min(base + bytes.len() as u64);
                if low < high {
                    out[(low - start) as usize..(high - start) as usize]
                        .copy_from_slice(&bytes[(low - base) as usize..(high - base) as usize]);
                }
            }
            self.bytes_read += length;
            Ok(length)
        }
    }
    let fixture = fixture(64, Endian::Big);
    let offset = (1_u64 << 32) + 0x1000;
    let record = fixture.symbols as u64;
    let target = fixture.addresses[0] + 4 - 0x1000;
    let mut prefix = vec![0; 40];
    put32(&mut prefix, 0, 0xcafe_babf, Endian::Little);
    put32(&mut prefix, 4, 1, Endian::Little);
    put32(&mut prefix, 8, 0x0100_000c, Endian::Little);
    put64(&mut prefix, 16, offset, Endian::Little);
    put64(&mut prefix, 24, fixture.bytes.len() as u64, Endian::Little);
    put32(&mut prefix, 32, 12, Endian::Little);
    let mut document = Document::new(Sparse {
        prefix,
        member: fixture.bytes,
        member_offset: offset,
        bytes_read: 0,
    });
    let image = formats::inspect(&mut document, None).unwrap().unwrap();
    let table = crate::symbols::inspect(&mut document, &image).unwrap();
    let symbol = named(&table, "_entry");
    assert_eq!(symbol.record.offset.get(), offset + record);
    assert_eq!(
        symbol.target.file_offset(&image),
        Ok(FileOffset::new(offset + target))
    );
    assert!(document.source().bytes_read < 16 * 1024);
}
