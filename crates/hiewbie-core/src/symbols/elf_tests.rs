//! Generated integer-field fixtures; no executable samples or third-party code.

use super::*;
use crate::instructions::DecodeMode;
use crate::{Command, Document, Editor, FileOffset, MemorySource, formats, symbols};

const BASE: u64 = 0x40_0000;
const TEXT: usize = 0x200;
const STRINGS: usize = 0x400;
const SYMBOLS: usize = 0xc00;
const SECTIONS: usize = 0x1000;

#[derive(Clone, Copy)]
struct Spec<'a> {
    name: &'a [u8],
    value: u64,
    size: u64,
    info: u8,
    other: u8,
    section: u16,
}

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

struct Fixture {
    bytes: Vec<u8>,
    bits: u8,
    endian: Endian,
}

impl Fixture {
    fn section(&self, index: usize) -> usize {
        SECTIONS + index * if self.bits == 64 { 64 } else { 40 }
    }

    fn entry_size(&self) -> usize {
        if self.bits == 64 { 24 } else { 16 }
    }

    fn entry(&self, index: usize) -> usize {
        SYMBOLS + index * self.entry_size()
    }

    fn section_word(&mut self, index: usize, at32: usize, at64: usize, value: u64) {
        let at = self.section(index);
        if self.bits == 64 {
            put64(&mut self.bytes, at + at64, value, self.endian);
        } else {
            put32(&mut self.bytes, at + at32, value as u32, self.endian);
        }
    }

    fn section_u32(&mut self, index: usize, at32: usize, at64: usize, value: u32) {
        let at = self.section(index) + if self.bits == 64 { at64 } else { at32 };
        put32(&mut self.bytes, at, value, self.endian);
    }

    fn symbol_name(&mut self, index: usize, value: u32) {
        let at = self.entry(index);
        put32(&mut self.bytes, at, value, self.endian);
    }

    fn symbol_section(&mut self, index: usize, value: u16) {
        let at = self.entry(index) + if self.bits == 64 { 6 } else { 14 };
        put16(&mut self.bytes, at, value, self.endian);
    }

    fn inspect(self) -> Result<(Executable, SymbolTable), FormatError> {
        let mut document = Document::new(MemorySource::new(self.bytes));
        let image = formats::inspect(&mut document, None)?.expect("ELF fixture recognized");
        let table = symbols::inspect(&mut document, &image)?;
        Ok((image, table))
    }
}

fn base_specs() -> Vec<Spec<'static>> {
    vec![
        Spec {
            name: b"local",
            value: BASE + TEXT as u64,
            size: 4,
            info: 2,
            other: 0,
            section: 1,
        },
        Spec {
            name: b"puts",
            value: 0,
            size: 0,
            info: 0x12,
            other: 0,
            section: 0,
        },
        Spec {
            name: b"start",
            value: BASE + TEXT as u64 + 8,
            size: 8,
            info: 0x12,
            other: 0,
            section: 1,
        },
        Spec {
            name: b"weak",
            value: BASE + TEXT as u64 + 16,
            size: 8,
            info: 0x22,
            other: 0,
            section: 1,
        },
        Spec {
            name: b"hidden",
            value: BASE + TEXT as u64 + 24,
            size: 8,
            info: 0x12,
            other: 2,
            section: 1,
        },
        Spec {
            name: b"protected",
            value: BASE + TEXT as u64 + 32,
            size: 8,
            info: 0x12,
            other: 3,
            section: 1,
        },
        Spec {
            name: b"tls",
            value: 40,
            size: 4,
            info: 0x16,
            other: 0,
            section: 1,
        },
        Spec {
            name: b"bss",
            value: BASE + 0x280,
            size: 4,
            info: 0x11,
            other: 0,
            section: 2,
        },
    ]
}

fn build_fixture(bits: u8, endian: Endian, dynamic: bool, specs: &[Spec<'_>]) -> Fixture {
    let mut fixture = Fixture {
        bytes: vec![0; 0x1400],
        bits,
        endian,
    };
    fixture.bytes[..4].copy_from_slice(b"\x7fELF");
    fixture.bytes[4] = if bits == 64 { 2 } else { 1 };
    fixture.bytes[5] = if endian == Endian::Little { 1 } else { 2 };
    fixture.bytes[6] = 1;
    put16(&mut fixture.bytes, 16, 2, endian);
    put16(
        &mut fixture.bytes,
        18,
        if bits == 64 { 62 } else { 3 },
        endian,
    );
    put32(&mut fixture.bytes, 20, 1, endian);
    let program = if bits == 64 { 64 } else { 52 };
    if bits == 64 {
        put64(&mut fixture.bytes, 24, BASE + TEXT as u64, endian);
        put64(&mut fixture.bytes, 32, program as u64, endian);
        put64(&mut fixture.bytes, 40, SECTIONS as u64, endian);
        put16(&mut fixture.bytes, 52, 64, endian);
        put16(&mut fixture.bytes, 54, 56, endian);
        put16(&mut fixture.bytes, 56, 1, endian);
        put16(&mut fixture.bytes, 58, 64, endian);
        put16(&mut fixture.bytes, 60, 5, endian);
        put32(&mut fixture.bytes, program, 1, endian);
        put32(&mut fixture.bytes, program + 4, 5, endian);
        put64(&mut fixture.bytes, program + 16, BASE, endian);
        put64(&mut fixture.bytes, program + 32, 0x280, endian);
        put64(&mut fixture.bytes, program + 40, 0x300, endian);
        put64(&mut fixture.bytes, program + 48, 0x1000, endian);
    } else {
        put32(&mut fixture.bytes, 24, BASE as u32 + TEXT as u32, endian);
        put32(&mut fixture.bytes, 28, program as u32, endian);
        put32(&mut fixture.bytes, 32, SECTIONS as u32, endian);
        put16(&mut fixture.bytes, 40, 52, endian);
        put16(&mut fixture.bytes, 42, 32, endian);
        put16(&mut fixture.bytes, 44, 1, endian);
        put16(&mut fixture.bytes, 46, 40, endian);
        put16(&mut fixture.bytes, 48, 5, endian);
        put32(&mut fixture.bytes, program, 1, endian);
        put32(&mut fixture.bytes, program + 8, BASE as u32, endian);
        put32(&mut fixture.bytes, program + 16, 0x280, endian);
        put32(&mut fixture.bytes, program + 20, 0x300, endian);
        put32(&mut fixture.bytes, program + 24, 5, endian);
        put32(&mut fixture.bytes, program + 28, 0x1000, endian);
    }
    fixture.bytes[TEXT..TEXT + 0x80].fill(0x90);
    fixture.section_u32(1, 4, 4, 1);
    fixture.section_word(1, 8, 8, SHF_ALLOC | 4);
    fixture.section_word(1, 12, 16, BASE + TEXT as u64);
    fixture.section_word(1, 16, 24, TEXT as u64);
    fixture.section_word(1, 20, 32, 0x80);
    fixture.section_u32(2, 4, 4, SHT_NOBITS);
    fixture.section_word(2, 8, 8, SHF_ALLOC | 1);
    fixture.section_word(2, 12, 16, BASE + 0x280);
    fixture.section_word(2, 16, 24, 0x280);
    fixture.section_word(2, 20, 32, 0x80);
    fixture.section_u32(3, 4, 4, SHT_STRTAB);
    fixture.section_word(3, 16, 24, STRINGS as u64);
    fixture.section_u32(4, 4, 4, if dynamic { SHT_DYNSYM } else { SHT_SYMTAB });
    fixture.section_word(4, 16, 24, SYMBOLS as u64);
    let entry_size = fixture.entry_size();
    fixture.section_word(4, 20, 32, ((specs.len() + 1) * entry_size) as u64);
    fixture.section_word(4, 36, 56, entry_size as u64);
    fixture.section_u32(4, 24, 40, 3);
    fixture.section_u32(
        4,
        28,
        44,
        1 + specs.iter().take_while(|spec| spec.info >> 4 == 0).count() as u32,
    );
    let mut next_string = 1;
    for (index, spec) in specs.iter().enumerate() {
        let name = if spec.name.is_empty() {
            0
        } else {
            next_string as u32
        };
        if !spec.name.is_empty() {
            let start = STRINGS + next_string;
            fixture.bytes[start..start + spec.name.len()].copy_from_slice(spec.name);
            next_string += spec.name.len() + 1;
        }
        let at = fixture.entry(index + 1);
        put32(&mut fixture.bytes, at, name, endian);
        if bits == 64 {
            fixture.bytes[at + 4] = spec.info;
            fixture.bytes[at + 5] = spec.other;
            put16(&mut fixture.bytes, at + 6, spec.section, endian);
            put64(&mut fixture.bytes, at + 8, spec.value, endian);
            put64(&mut fixture.bytes, at + 16, spec.size, endian);
        } else {
            put32(&mut fixture.bytes, at + 4, spec.value as u32, endian);
            put32(&mut fixture.bytes, at + 8, spec.size as u32, endian);
            fixture.bytes[at + 12] = spec.info;
            fixture.bytes[at + 13] = spec.other;
            put16(&mut fixture.bytes, at + 14, spec.section, endian);
        }
    }
    fixture.section_word(3, 20, 32, next_string as u64);
    fixture
}

fn named<'a>(table: &'a SymbolTable, name: &str) -> &'a Symbol {
    table
        .entries
        .iter()
        .find(|symbol| symbol.name == name)
        .expect("named fixture symbol")
}

#[test]
fn elf_widths_orders_and_table_kinds_preserve_symbol_semantics() {
    for bits in [32, 64] {
        for endian in [Endian::Little, Endian::Big] {
            for dynamic in [false, true] {
                let fixture = build_fixture(bits, endian, dynamic, &base_specs());
                let entry_size = fixture.entry_size() as u64;
                let (image, table) = fixture.inspect().unwrap();
                assert_eq!(table.entries.len(), 8);
                assert_eq!(named(&table, "local").kind, SymbolKind::Defined);
                assert_eq!(named(&table, "puts").kind, SymbolKind::Import);
                assert_eq!(named(&table, "puts").target, SymbolTarget::Unavailable);
                for name in ["start", "weak", "protected", "tls", "bss"] {
                    assert_eq!(
                        named(&table, name).kind,
                        if dynamic {
                            SymbolKind::Export
                        } else {
                            SymbolKind::Defined
                        }
                    );
                }
                assert_eq!(named(&table, "hidden").kind, SymbolKind::Defined);
                assert_eq!(named(&table, "tls").target, SymbolTarget::Unavailable);
                assert_eq!(named(&table, "bss").target, SymbolTarget::Unavailable);
                assert!(named(&table, "tls").detail.contains("TLS offset"));
                assert!(named(&table, "bss").detail.contains("zero-fill"));
                let start = named(&table, "start");
                assert_eq!(
                    start.target,
                    SymbolTarget::Virtual(VirtualAddress::new(BASE + TEXT as u64 + 8))
                );
                assert_eq!(
                    start.target.file_offset(&image).unwrap(),
                    FileOffset::new(TEXT as u64 + 8)
                );
                assert_eq!(
                    start.record.offset,
                    FileOffset::new(SYMBOLS as u64 + 3 * entry_size)
                );
                assert_eq!(start.record.len, entry_size);
                assert_eq!(start.size, Some(8));
                assert!(
                    table
                        .notes
                        .iter()
                        .any(|note| note.contains("library bindings"))
                );
            }
        }
    }
}

#[test]
fn relocatable_symbols_use_section_offsets_and_special_values_have_no_target() {
    let specs = [
        Spec {
            name: b"file.c",
            value: 0,
            size: 0,
            info: 4,
            other: 0,
            section: SHN_ABS,
        },
        Spec {
            name: b"relative",
            value: 8,
            size: 4,
            info: 0x12,
            other: 0,
            section: 1,
        },
        Spec {
            name: b"absolute",
            value: BASE + TEXT as u64,
            size: 0,
            info: 0x10,
            other: 0,
            section: SHN_ABS,
        },
        Spec {
            name: b"common",
            value: 16,
            size: 8,
            info: 0x15,
            other: 0,
            section: SHN_COMMON,
        },
        Spec {
            name: b"tls",
            value: 8,
            size: 4,
            info: 0x16,
            other: 0,
            section: 1,
        },
        Spec {
            name: b"end",
            value: 0x80,
            size: 0,
            info: 0x10,
            other: 0,
            section: 1,
        },
        Spec {
            name: b"oversized",
            value: 0x7c,
            size: 8,
            info: 0x11,
            other: 0,
            section: 1,
        },
    ];
    for bits in [32, 64] {
        for endian in [Endian::Little, Endian::Big] {
            let mut fixture = build_fixture(bits, endian, false, &specs);
            put16(&mut fixture.bytes, 16, 1, endian);
            put16(
                &mut fixture.bytes,
                if bits == 64 { 56 } else { 44 },
                0,
                endian,
            );
            let (_, table) = fixture.inspect().unwrap();
            assert_eq!(
                named(&table, "relative").target,
                SymbolTarget::File(FileOffset::new(TEXT as u64 + 8))
            );
            for name in ["file.c", "absolute", "common", "tls", "end", "oversized"] {
                assert_eq!(
                    named(&table, name).target,
                    SymbolTarget::Unavailable,
                    "{name}"
                );
            }
            assert!(named(&table, "common").detail.contains("alignment"));
        }
    }
}

#[test]
fn thumb_function_state_is_separate_from_file_address() {
    let mut specs = base_specs();
    specs[2].value |= 1;
    for endian in [Endian::Little, Endian::Big] {
        let mut fixture = build_fixture(32, endian, true, &specs);
        put16(&mut fixture.bytes, 18, 40, endian);
        let (image, table) = fixture.inspect().unwrap();
        let start = named(&table, "start");
        assert_eq!(
            start.target,
            SymbolTarget::Thumb(VirtualAddress::new(BASE + TEXT as u64 + 9))
        );
        assert_eq!(
            start.target.file_offset(&image).unwrap(),
            FileOffset::new(TEXT as u64 + 8)
        );
    }
    specs[2].value = 9;
    let mut fixture = build_fixture(32, Endian::Little, false, &specs);
    put16(&mut fixture.bytes, 18, 40, Endian::Little);
    put16(&mut fixture.bytes, 16, 1, Endian::Little);
    let (_, table) = fixture.inspect().unwrap();
    assert_eq!(
        named(&table, "start").target,
        SymbolTarget::File(FileOffset::new(TEXT as u64 + 8))
    );
    assert!(named(&table, "start").detail.contains("Thumb"));
}

#[test]
fn undefined_nonzero_value_never_becomes_a_file_target() {
    let mut specs = base_specs();
    specs[1].value = BASE + TEXT as u64;
    specs[1].info = 0x22;
    let (_, table) = build_fixture(64, Endian::Little, true, &specs)
        .inspect()
        .unwrap();
    assert_eq!(named(&table, "puts").kind, SymbolKind::Import);
    assert_eq!(named(&table, "puts").target, SymbolTarget::Unavailable);
}

#[test]
fn mismatched_nonallocated_and_reserved_sections_do_not_fake_a_mapping() {
    for change in 0..4 {
        let mut fixture = build_fixture(64, Endian::Little, true, &base_specs());
        match change {
            0 => fixture.section_word(1, 16, 24, TEXT as u64 + 1),
            1 => fixture.section_word(1, 8, 8, 0),
            2 => fixture.symbol_section(3, 0xff10),
            _ => fixture.section_word(1, 8, 8, SHF_ALLOC | SHF_COMPRESSED),
        }
        let (_, table) = fixture.inspect().unwrap();
        assert_eq!(named(&table, "start").target, SymbolTarget::Unavailable);
    }
}

#[test]
fn malformed_symbols_do_not_break_header_or_raw_access() {
    for case in 0..12 {
        let mut fixture = build_fixture(64, Endian::Little, true, &base_specs());
        match case {
            0 => fixture.section_word(4, 36, 56, 0),
            1 => fixture.section_word(4, 20, 32, 25),
            2 => fixture.section_u32(4, 24, 40, 99),
            3 => fixture.section_u32(4, 24, 40, 1),
            4 => fixture.symbol_name(3, u32::MAX),
            5 => fixture.symbol_section(3, 99),
            6 => fixture.bytes[STRINGS] = b'x',
            7 => fixture.section_word(3, 16, 24, u64::MAX),
            8 => fixture.section_u32(4, 28, 44, 99),
            9 => fixture.symbol_name(0, 1),
            10 => fixture.section_word(4, 16, 24, 0x13f0),
            _ => fixture.section_u32(4, 28, 44, 0),
        }
        let mut document = Document::new(MemorySource::new(fixture.bytes));
        let image = formats::inspect(&mut document, None).unwrap().unwrap();
        assert!(
            symbols::inspect(&mut document, &image).is_err(),
            "case {case}"
        );
        assert_eq!(
            image.va_to_file(image.entry.unwrap()).unwrap(),
            FileOffset::new(TEXT as u64)
        );
        assert!(formats::inspect(&mut document, None).unwrap().is_some());
        let mut raw = [0; 1];
        assert_eq!(
            document
                .read_at(FileOffset::new(TEXT as u64), &mut raw)
                .unwrap(),
            1
        );
        assert_eq!(raw, [0x90]);
        assert_eq!(document.generation(), 0);
    }
}

#[test]
fn malformed_string_termination_and_local_partition_are_rejected() {
    let mut fixture = build_fixture(32, Endian::Big, false, &base_specs());
    let strings_size = u32_at(&fixture.bytes, fixture.section(3) + 20, Endian::Big) as usize;
    fixture.bytes[STRINGS + strings_size - 1] = b'x';
    assert!(matches!(
        fixture.inspect(),
        Err(FormatError::Invalid { .. })
    ));
    let mut fixture = build_fixture(32, Endian::Little, false, &base_specs());
    fixture.section_u32(4, 28, 44, 1);
    assert!(matches!(
        fixture.inspect(),
        Err(FormatError::Invalid { .. })
    ));
}

#[test]
fn name_limits_and_substring_indices_follow_bounded_string_table_rules() {
    for length in [
        super::super::MAX_SYMBOL_NAME_BYTES,
        super::super::MAX_SYMBOL_NAME_BYTES + 1,
    ] {
        let name = vec![b'x'; length];
        let specs = [Spec {
            name: &name,
            value: BASE + TEXT as u64,
            size: 1,
            info: 0x11,
            other: 0,
            section: 1,
        }];
        let result = build_fixture(64, Endian::Little, false, &specs).inspect();
        if length == super::super::MAX_SYMBOL_NAME_BYTES {
            assert_eq!(result.unwrap().1.entries[0].name.len(), length);
        } else {
            assert!(matches!(result, Err(FormatError::Limit { .. })));
        }
    }
    let mut fixture = build_fixture(64, Endian::Little, false, &base_specs());
    fixture.symbol_name(1, 3);
    let (_, table) = fixture.inspect().unwrap();
    assert_eq!(table.entries[0].name, "cal");
}

#[test]
fn non_utf8_and_control_bytes_are_preserved_for_presentation_escaping() {
    let specs = [Spec {
        name: b"bad\xff\n\x1b[31m",
        value: BASE + TEXT as u64,
        size: 1,
        info: 0x11,
        other: 0,
        section: 1,
    }];
    let (_, table) = build_fixture(64, Endian::Little, false, &specs)
        .inspect()
        .unwrap();
    assert_eq!(table.entries[0].name, "bad\u{fffd}\n\x1b[31m");
}

#[test]
fn count_limit_is_checked_before_symbol_allocation_or_table_read() {
    let mut fixture = build_fixture(64, Endian::Little, true, &base_specs());
    fixture.section_word(4, 20, 32, (MAX_SYMBOLS as u64 + 1) * 24);
    assert!(matches!(
        fixture.inspect(),
        Err(FormatError::Limit {
            resource: "ELF symbol records",
            limit: MAX_SYMBOLS
        })
    ));
}

#[test]
fn record_limit_is_shared_across_symbol_tables_including_unnamed_entries() {
    let mut fixture = build_fixture(64, Endian::Little, false, &[]);
    put16(&mut fixture.bytes, 60, 6, Endian::Little);
    let first_header = fixture.section(4);
    let second_header = fixture.section(5);
    fixture
        .bytes
        .copy_within(first_header..first_header + 64, second_header);
    fixture.bytes.resize(0x2000 + MAX_SYMBOLS * 24, 0);
    fixture.section_word(4, 16, 24, 0x2000);
    fixture.section_word(4, 20, 32, MAX_SYMBOLS as u64 * 24);
    fixture.section_u32(4, 28, 44, MAX_SYMBOLS as u32);
    fixture.section_word(5, 16, 24, 0x2000);
    assert!(matches!(
        fixture.inspect(),
        Err(FormatError::Limit {
            resource: "ELF symbol records",
            limit: MAX_SYMBOLS
        })
    ));
}

#[test]
fn extended_symbol_indices_and_compressed_tables_are_explicitly_unsupported() {
    let mut fixture = build_fixture(64, Endian::Little, true, &base_specs());
    fixture.symbol_section(3, SHN_XINDEX);
    assert!(matches!(
        fixture.inspect(),
        Err(FormatError::Unsupported(
            "ELF extended symbol section indices"
        ))
    ));
    for index in [3, 4] {
        let mut fixture = build_fixture(64, Endian::Little, true, &base_specs());
        fixture.section_word(index, 8, 8, SHF_COMPRESSED);
        assert!(matches!(
            fixture.inspect(),
            Err(FormatError::Unsupported(_))
        ));
    }
}

#[test]
fn sectionless_and_versioned_images_report_coverage_instead_of_false_completeness() {
    let mut fixture = build_fixture(64, Endian::Little, true, &base_specs());
    put64(&mut fixture.bytes, 40, 0, Endian::Little);
    put16(&mut fixture.bytes, 60, 0, Endian::Little);
    let (_, table) = fixture.inspect().unwrap();
    assert!(table.entries.is_empty());
    assert!(table.notes.iter().any(|note| note.contains("PT_DYNAMIC")));
    let mut fixture = build_fixture(64, Endian::Little, true, &base_specs());
    put16(&mut fixture.bytes, 60, 6, Endian::Little);
    fixture.section_u32(5, 4, 4, 0x6fff_ffff);
    let (_, table) = fixture.inspect().unwrap();
    assert!(
        table
            .notes
            .iter()
            .any(|note| note.contains("different unresolved symbol versions"))
    );
}

#[test]
fn padded_entries_use_declared_stride_without_reading_padding_as_symbols() {
    let specs = [Spec {
        name: b"padded",
        value: BASE + TEXT as u64,
        size: 4,
        info: 0x12,
        other: 0,
        section: 1,
    }];
    for bits in [32, 64] {
        let mut fixture = build_fixture(bits, Endian::Little, false, &specs);
        let normal = fixture.entry_size();
        let stride = normal + 8;
        fixture
            .bytes
            .copy_within(SYMBOLS + normal..SYMBOLS + normal * 2, SYMBOLS + stride);
        fixture.bytes[SYMBOLS + normal..SYMBOLS + stride].fill(0xff);
        fixture.section_word(4, 20, 32, (2 * stride) as u64);
        fixture.section_word(4, 36, 56, stride as u64);
        let (_, table) = fixture.inspect().unwrap();
        assert_eq!(table.entries.len(), 1);
        assert_eq!(table.entries[0].name, "padded");
        assert_eq!(
            table.entries[0].record.offset,
            FileOffset::new((SYMBOLS + stride) as u64)
        );
        assert_eq!(table.entries[0].record.len, normal as u64);
    }
}

fn arm_navigation_fixture(thumb_entry: bool) -> Fixture {
    let specs = [
        Spec {
            name: b"arm_fn",
            value: BASE + TEXT as u64,
            size: 4,
            info: 0x12,
            other: 0,
            section: 1,
        },
        Spec {
            name: b"thumb_fn",
            value: BASE + TEXT as u64 + 9,
            size: 4,
            info: 0x12,
            other: 0,
            section: 1,
        },
        Spec {
            name: b"data",
            value: BASE + TEXT as u64 + 32,
            size: 4,
            info: 0x11,
            other: 0,
            section: 1,
        },
        Spec {
            name: b"missing",
            value: 0,
            size: 0,
            info: 0x12,
            other: 0,
            section: 0,
        },
    ];
    let mut fixture = build_fixture(32, Endian::Little, true, &specs);
    put16(&mut fixture.bytes, 18, 40, Endian::Little);
    put32(
        &mut fixture.bytes,
        24,
        BASE as u32 + TEXT as u32 + if thumb_entry { 9 } else { 0 },
        Endian::Little,
    );
    put32(&mut fixture.bytes, TEXT, 0xe1a0_0000, Endian::Little);
    fixture.bytes[TEXT + 8..TEXT + 12].copy_from_slice(&[0, 0xbf, 0x70, 0x47]);
    fixture
}

fn navigation_editor(fixture: Fixture) -> Editor<MemorySource> {
    Editor::new(Document::new(MemorySource::new(fixture.bytes)), false)
}

fn symbol_index(editor: &mut Editor<MemorySource>, name: &str) -> usize {
    editor.refresh_symbols();
    editor
        .symbols()
        .unwrap()
        .entries
        .iter()
        .position(|symbol| symbol.name == name)
        .unwrap()
}

fn follow_symbol(editor: &mut Editor<MemorySource>, name: &str) {
    let index = symbol_index(editor, name);
    editor.dispatch(Command::GotoSymbol(index)).unwrap();
}

#[test]
fn symbol_navigation_switches_arm_and_thumb_without_creating_user_overrides() {
    for thumb_entry in [false, true] {
        let mut editor = navigation_editor(arm_navigation_fixture(thumb_entry));
        follow_symbol(&mut editor, "thumb_fn");
        assert_eq!(editor.cursor().get(), TEXT as u64 + 8);
        assert_eq!(editor.decode_context().mode, Some(DecodeMode::Thumb));
        assert!(!editor.decode_context().overridden);
        editor.dispatch(Command::MoveCode(1)).unwrap();
        assert_eq!(editor.cursor().get(), TEXT as u64 + 10);
        assert_eq!(editor.decode_context().mode, Some(DecodeMode::Thumb));
        editor.dispatch(Command::Move(-2)).unwrap();
        assert_eq!(editor.decode_context().mode, Some(DecodeMode::Thumb));
        follow_symbol(&mut editor, "arm_fn");
        assert_eq!(editor.cursor().get(), TEXT as u64);
        assert_eq!(editor.decode_context().mode, Some(DecodeMode::Arm));
        assert!(!editor.decode_context().overridden);
        let arm = named(editor.symbols().unwrap(), "arm_fn");
        assert_eq!(
            arm.target,
            SymbolTarget::Arm(VirtualAddress::new(BASE + TEXT as u64))
        );
        editor.dispatch(Command::GotoEntry).unwrap();
        assert_eq!(
            editor.cursor().get(),
            TEXT as u64 + if thumb_entry { 8 } else { 0 }
        );
        assert_eq!(
            editor.decode_context().mode,
            Some(if thumb_entry {
                DecodeMode::Thumb
            } else {
                DecodeMode::Arm
            })
        );
        assert!(!editor.document().is_dirty());
    }
}

#[test]
fn explicit_decoder_mode_wins_over_symbol_hints_and_auto_clears_the_hint() {
    for mode in [DecodeMode::Arm, DecodeMode::Thumb, DecodeMode::X86_64] {
        let mut editor = navigation_editor(arm_navigation_fixture(false));
        editor.dispatch(Command::SetDecodeMode(Some(mode))).unwrap();
        for name in ["thumb_fn", "arm_fn"] {
            follow_symbol(&mut editor, name);
            assert_eq!(editor.decode_context().mode, Some(mode));
            assert!(editor.decode_context().overridden);
        }
        follow_symbol(&mut editor, "thumb_fn");
        editor.dispatch(Command::SetDecodeMode(None)).unwrap();
        assert_eq!(editor.decode_context().mode, Some(DecodeMode::Arm));
        assert!(!editor.decode_context().overridden);
        editor
            .dispatch(Command::SetEndian(Some(Endian::Big)))
            .unwrap();
        follow_symbol(&mut editor, "thumb_fn");
        assert_eq!(editor.decode_context().endian, Endian::Big);
        assert_eq!(editor.decode_context().mode, Some(DecodeMode::Thumb));
    }
}

#[test]
fn successful_untyped_symbol_and_absolute_navigation_clear_only_the_hint() {
    for command in [
        Command::Goto(FileOffset::new(TEXT as u64)),
        Command::GotoVirtual(VirtualAddress::new(BASE + TEXT as u64)),
        Command::GotoEntry,
        Command::GotoRegion(0),
        Command::GotoSymbol(2),
        Command::GotoSymbolRecord(1),
        Command::SetDecodeMode(None),
    ] {
        let mut editor = navigation_editor(arm_navigation_fixture(false));
        follow_symbol(&mut editor, "thumb_fn");
        editor.dispatch(command).unwrap();
        assert_eq!(editor.decode_context().mode, Some(DecodeMode::Arm));
        assert!(!editor.decode_context().overridden);
    }
}

#[test]
fn failed_navigation_preserves_symbol_hint_cursor_and_explicit_override() {
    for explicit in [None, Some(DecodeMode::X86_64)] {
        let mut fixture = arm_navigation_fixture(false);
        put32(&mut fixture.bytes, 24, 0xdead_0000, Endian::Little);
        let mut editor = navigation_editor(fixture);
        editor.dispatch(Command::SetDecodeMode(explicit)).unwrap();
        follow_symbol(&mut editor, "thumb_fn");
        let cursor = editor.cursor();
        let context = editor.decode_context();
        for command in [
            Command::GotoSymbol(3),
            Command::GotoSymbol(usize::MAX),
            Command::GotoSymbolRecord(usize::MAX),
            Command::GotoVirtual(VirtualAddress::new(u64::MAX)),
            Command::GotoRegion(usize::MAX),
            Command::GotoEntry,
            Command::SelectSlice(0),
            Command::FollowBranch(0),
            Command::ReturnBranch,
        ] {
            assert!(editor.dispatch(command).is_err());
            assert_eq!(editor.cursor(), cursor);
            assert_eq!(editor.decode_context(), context);
        }
    }
}

#[test]
fn logical_generation_and_raw_invalidation_discard_symbol_hints() {
    let mut editor = navigation_editor(arm_navigation_fixture(false));
    editor.dispatch(Command::BeginEdit).unwrap();
    follow_symbol(&mut editor, "thumb_fn");
    editor
        .document_mut()
        .overwrite(FileOffset::new(0x300), b"x")
        .unwrap();
    assert_eq!(editor.decode_context().mode, None);
    editor.refresh_analysis();
    assert_eq!(editor.decode_context().mode, Some(DecodeMode::Arm));
    assert!(editor.symbols().is_none());
    follow_symbol(&mut editor, "thumb_fn");
    editor.dispatch(Command::Undo).unwrap();
    editor.refresh_analysis();
    assert_eq!(editor.decode_context().mode, Some(DecodeMode::Arm));
    follow_symbol(&mut editor, "thumb_fn");
    editor.dispatch(Command::SetRaw(true)).unwrap();
    editor.refresh_analysis();
    assert_eq!(editor.decode_context().mode, None);
    editor.dispatch(Command::SetRaw(false)).unwrap();
    editor.refresh_analysis();
    assert_eq!(editor.decode_context().mode, Some(DecodeMode::Arm));
    follow_symbol(&mut editor, "thumb_fn");
    editor.complete_save(MemorySource::new(arm_navigation_fixture(false).bytes));
    editor.refresh_analysis();
    assert_eq!(editor.decode_context().mode, Some(DecodeMode::Arm));
}

#[test]
fn branch_return_restores_automatic_symbol_mode_without_turning_it_into_override() {
    for start_mode in [DecodeMode::Arm, DecodeMode::Thumb] {
        let mut fixture = arm_navigation_fixture(start_mode == DecodeMode::Arm);
        let (name, destination, exchanged) = if start_mode == DecodeMode::Arm {
            put32(&mut fixture.bytes, TEXT, 0xfa00_0000, Endian::Little);
            ("arm_fn", TEXT + 8, DecodeMode::Thumb)
        } else {
            fixture.bytes[TEXT + 8..TEXT + 12].copy_from_slice(&[0, 0xf0, 0, 0xe8]);
            put32(&mut fixture.bytes, TEXT + 12, 0xe1a0_0000, Endian::Little);
            ("thumb_fn", TEXT + 12, DecodeMode::Arm)
        };
        let mut editor = navigation_editor(fixture);
        follow_symbol(&mut editor, name);
        let source = editor.cursor();
        let before = editor.decode_context();
        editor.code_rows(1).unwrap();
        editor.dispatch(Command::FollowBranch(1)).unwrap();
        assert_eq!(editor.cursor().get(), destination as u64);
        assert_eq!(editor.decode_context().mode, Some(exchanged));
        editor.dispatch(Command::ReturnBranch).unwrap();
        assert_eq!(editor.cursor(), source);
        assert_eq!(editor.decode_context(), before);
        assert!(!editor.decode_context().overridden);
        editor.dispatch(Command::GotoEntry).unwrap();
        assert_eq!(editor.decode_context().mode, Some(exchanged));
    }
}

#[test]
fn same_isa_branch_keeps_hint_and_changed_generation_prevents_hint_resurrection() {
    let mut fixture = arm_navigation_fixture(true);
    put32(&mut fixture.bytes, TEXT, 0xea00_0000, Endian::Little);
    put32(&mut fixture.bytes, TEXT + 8, 0xe1a0_0000, Endian::Little);
    let mut editor = navigation_editor(fixture);
    follow_symbol(&mut editor, "arm_fn");
    editor.code_rows(1).unwrap();
    editor.dispatch(Command::FollowBranch(1)).unwrap();
    assert_eq!(editor.decode_context().mode, Some(DecodeMode::Arm));
    assert!(!editor.decode_context().overridden);
    editor.dispatch(Command::ReturnBranch).unwrap();
    assert_eq!(editor.decode_context().mode, Some(DecodeMode::Arm));
    assert!(!editor.decode_context().overridden);
    editor.dispatch(Command::FollowBranch(1)).unwrap();
    editor
        .document_mut()
        .overwrite(FileOffset::new(0x300), b"x")
        .unwrap();
    editor.dispatch(Command::ReturnBranch).unwrap();
    assert_eq!(editor.decode_context().mode, Some(DecodeMode::Thumb));
    assert!(!editor.decode_context().overridden);
}

#[test]
fn every_symbol_target_requires_the_mapped_byte_to_remain_inside_the_slice() {
    let (mut image, _) = arm_navigation_fixture(false).inspect().unwrap();
    let address = VirtualAddress::new(BASE + TEXT as u64);
    let targets = [
        SymbolTarget::Virtual(address),
        SymbolTarget::Arm(address),
        SymbolTarget::Thumb(VirtualAddress::new(address.get() | 1)),
        SymbolTarget::ImportSlot(address),
        SymbolTarget::File(FileOffset::new(TEXT as u64)),
    ];
    for target in targets {
        assert_eq!(
            target.file_offset(&image).unwrap(),
            FileOffset::new(TEXT as u64)
        );
    }
    image.slice.len = TEXT as u64;
    for target in targets {
        assert_eq!(
            target.file_offset(&image),
            Err(formats::AddressError::Unmapped)
        );
    }
    image.slice.offset = FileOffset::new(TEXT as u64 + 1);
    image.slice.len = 0x100;
    for target in targets {
        assert_eq!(
            target.file_offset(&image),
            Err(formats::AddressError::Unmapped)
        );
    }
}
