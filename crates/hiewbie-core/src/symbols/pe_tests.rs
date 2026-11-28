//! Small PE images assembled from documented fields. No program binaries or
//! third-party fixture data are distributed.

use super::*;
use crate::formats::{self, AddressError};
use crate::{Document, FileOffset, MemorySource};

const OPTIONAL: usize = 0x98;

fn put16(bytes: &mut [u8], at: usize, value: u16) {
    bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
}

fn put32(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn put64(bytes: &mut [u8], at: usize, value: u64) {
    bytes[at..at + 8].copy_from_slice(&value.to_le_bytes());
}

fn text(bytes: &mut [u8], at: usize, value: &[u8]) {
    bytes[at..at + value.len()].copy_from_slice(value);
    bytes[at + value.len()] = 0;
}

fn dir(bytes: &mut [u8], bits: u8, index: usize, rva: u32, size: u32) {
    let at = OPTIONAL + if bits == 64 { 112 } else { 96 } + index * 8;
    put32(bytes, at, rva);
    put32(bytes, at + 4, size);
}

fn set_thunk(bytes: &mut [u8], bits: u8, at: usize, value: u64) {
    if bits == 64 {
        put64(bytes, at, value);
    } else {
        put32(bytes, at, value as u32);
    }
}

fn base(bits: u8) -> u64 {
    if bits == 64 { 0x1_4000_0000 } else { 0x40_0000 }
}

fn resize_section(bytes: &mut Vec<u8>, bits: u8, size: u32) {
    bytes.resize(0x200 + size as usize, 0);
    let section = OPTIONAL + if bits == 64 { 240 } else { 224 };
    put32(bytes, section + 8, size);
    put32(bytes, section + 16, size);
    put32(
        bytes,
        OPTIONAL + 56,
        0x1000 + size.div_ceil(0x1000) * 0x1000,
    );
}

fn fixture(bits: u8) -> Vec<u8> {
    let mut bytes = vec![0; 0xa00];
    bytes[..2].copy_from_slice(b"MZ");
    put32(&mut bytes, 0x3c, 0x80);
    bytes[0x80..0x84].copy_from_slice(b"PE\0\0");
    put16(&mut bytes, 0x84, if bits == 64 { 0x8664 } else { 0x14c });
    put16(&mut bytes, 0x86, 1);
    let optional_size = if bits == 64 { 240 } else { 224 };
    put16(&mut bytes, 0x94, optional_size);
    put16(&mut bytes, OPTIONAL, if bits == 64 { 0x20b } else { 0x10b });
    put32(&mut bytes, OPTIONAL + 16, 0x1600);
    if bits == 64 {
        put64(&mut bytes, OPTIONAL + 24, base(bits));
    } else {
        put32(&mut bytes, OPTIONAL + 28, base(bits) as u32);
    }
    put32(&mut bytes, OPTIONAL + 32, 0x1000);
    put32(&mut bytes, OPTIONAL + 36, 0x200);
    put32(&mut bytes, OPTIONAL + 56, 0x2000);
    put32(&mut bytes, OPTIONAL + 60, 0x200);
    put32(&mut bytes, OPTIONAL + if bits == 64 { 108 } else { 92 }, 16);
    let section = OPTIONAL + usize::from(optional_size);
    bytes[section..section + 5].copy_from_slice(b".data");
    put32(&mut bytes, section + 8, 0x800);
    put32(&mut bytes, section + 12, 0x1000);
    put32(&mut bytes, section + 16, 0x800);
    put32(&mut bytes, section + 20, 0x200);
    put32(&mut bytes, section + 36, 0xc000_0040);

    dir(&mut bytes, bits, 0, 0x1000, 0x180);
    put32(&mut bytes, 0x20c, 0x10c0);
    put32(&mut bytes, 0x210, 7);
    put32(&mut bytes, 0x214, 5);
    put32(&mut bytes, 0x218, 4);
    put32(&mut bytes, 0x21c, 0x1040);
    put32(&mut bytes, 0x220, 0x1060);
    put32(&mut bytes, 0x224, 0x1070);
    for (index, value) in [0x1600, 0x1610, 0x1100, 0, 0x1620].into_iter().enumerate() {
        put32(&mut bytes, 0x240 + index * 4, value);
    }
    for (index, (name_rva, ordinal_index)) in [(0x1080, 0), (0x1086, 0), (0x108c, 2), (0x1096, 4)]
        .into_iter()
        .enumerate()
    {
        put32(&mut bytes, 0x260 + index * 4, name_rva);
        put16(&mut bytes, 0x270 + index * 2, ordinal_index);
    }
    text(&mut bytes, 0x280, b"Alias");
    text(&mut bytes, 0x286, b"Alpha");
    text(&mut bytes, 0x28c, b"Forwarded");
    text(&mut bytes, 0x296, b"Zulu");
    text(&mut bytes, 0x2c0, b"fixture.dll");
    text(&mut bytes, 0x300, b"other.#12");

    dir(&mut bytes, bits, 1, 0x1200, 40);
    put32(&mut bytes, 0x400, 0x1300);
    put32(&mut bytes, 0x40c, 0x1380);
    put32(&mut bytes, 0x410, 0x1340);
    let width = if bits == 64 { 8 } else { 4 };
    let ordinal_flag = if bits == 64 { 1u64 << 63 } else { 1u64 << 31 };
    for at in [0x500, 0x540] {
        set_thunk(&mut bytes, bits, at, 0x13a0);
        set_thunk(&mut bytes, bits, at + width, ordinal_flag | 0xfedc);
    }
    text(&mut bytes, 0x580, b"KERNEL32.dll");
    put16(&mut bytes, 0x5a0, 17);
    text(&mut bytes, 0x5a2, b"CreateFileA");
    bytes[0x800] = 0xc3;
    bytes
}

fn loaded(bytes: Vec<u8>) -> (Document<MemorySource>, Executable) {
    let mut document = Document::new(MemorySource::new(bytes));
    let image = formats::inspect(&mut document, None).unwrap().unwrap();
    (document, image)
}

fn inspect(bytes: Vec<u8>) -> Result<SymbolTable, FormatError> {
    let (mut document, image) = loaded(bytes);
    super::super::inspect(&mut document, &image)
}

fn invalid(bytes: Vec<u8>, expected: &str) {
    let error = inspect(bytes).unwrap_err();
    assert!(matches!(error, FormatError::Invalid { .. }), "{error}");
    assert!(error.to_string().contains(expected), "{error}");
}

#[test]
fn pe32_and_pe64_keep_aliases_ordinals_forwarders_and_iat_slots_distinct() {
    for bits in [32, 64] {
        let (mut document, image) = loaded(fixture(bits));
        let table = super::super::inspect(&mut document, &image).unwrap();
        let names: Vec<_> = table
            .entries
            .iter()
            .map(|symbol| symbol.name.as_str())
            .collect();
        assert_eq!(
            names,
            [
                "Alias",
                "Alpha",
                "Forwarded",
                "Zulu",
                "#8",
                "CreateFileA",
                "#65244"
            ]
        );
        assert_eq!(table.entries[0].ordinal, Some(7));
        assert_eq!(table.entries[1].ordinal, Some(7));
        assert_eq!(table.entries[0].record, table.entries[1].record);
        assert_eq!(table.entries[0].record.offset, FileOffset::new(0x240));
        assert_eq!(table.entries[0].record.len, 4);
        assert_eq!(table.entries[0].library.as_deref(), Some("fixture.dll"));
        assert_eq!(
            table.entries[0].target,
            SymbolTarget::Virtual(VirtualAddress::new(base(bits) + 0x1600))
        );
        assert_eq!(
            table.entries[0].target.file_offset(&image).unwrap(),
            FileOffset::new(0x800)
        );
        assert_eq!(table.entries[2].target, SymbolTarget::Unavailable);
        assert!(table.entries[2].detail.contains("other.#12"));
        assert_eq!(
            table.entries[2].target.file_offset(&image),
            Err(AddressError::Unmapped)
        );
        assert_eq!(table.entries[4].ordinal, Some(8));
        let imported = &table.entries[5];
        assert_eq!(imported.kind, SymbolKind::Import);
        assert_eq!(imported.library.as_deref(), Some("KERNEL32.dll"));
        assert_eq!(imported.ordinal, None);
        assert!(imported.detail.contains("hint 17"));
        assert_eq!(imported.record.offset, FileOffset::new(0x500));
        assert_eq!(imported.record.len, if bits == 64 { 8 } else { 4 });
        assert_eq!(
            imported.target,
            SymbolTarget::ImportSlot(VirtualAddress::new(base(bits) + 0x1340))
        );
        assert_eq!(
            imported.target.file_offset(&image).unwrap(),
            FileOffset::new(0x540)
        );
        assert_eq!(table.entries[6].ordinal, Some(0xfedc));
    }
}

#[test]
fn absent_directories_remain_an_empty_documented_result() {
    let mut bytes = fixture(64);
    dir(&mut bytes, 64, 0, 0, 0);
    dir(&mut bytes, 64, 1, 0, 0);
    let table = inspect(bytes).unwrap();
    assert!(table.entries.is_empty());
    assert!(table.notes.iter().any(|note| note.contains("debug/PDB")));
}

#[test]
fn unbound_iat_is_a_valid_fallback_when_the_lookup_table_is_absent() {
    for bits in [32, 64] {
        let mut bytes = fixture(bits);
        put32(&mut bytes, 0x400, 0);
        let table = inspect(bytes).unwrap();
        assert_eq!(table.entries[5].name, "CreateFileA");
        assert_eq!(table.entries[5].record.offset, FileOffset::new(0x540));
        assert!(
            table
                .notes
                .iter()
                .any(|note| note.contains("validated unbound IAT"))
        );
    }
}

#[test]
fn bound_iat_with_an_intact_lookup_table_never_becomes_a_code_target() {
    for bits in [32, 64] {
        let mut bytes = fixture(bits);
        put32(&mut bytes, 0x404, 1);
        set_thunk(&mut bytes, bits, 0x540, 0x7654_3210);
        let table = inspect(bytes).unwrap();
        assert_eq!(table.entries[5].name, "CreateFileA");
        assert_eq!(
            table.entries[5].target,
            SymbolTarget::ImportSlot(VirtualAddress::new(base(bits) + 0x1340))
        );
        assert!(
            table
                .notes
                .iter()
                .any(|note| note.contains("binding/forwarder-chain"))
        );
    }
}

#[test]
fn bound_only_imports_delay_imports_and_coff_symbols_have_explicit_gaps() {
    let mut bytes = fixture(64);
    put32(&mut bytes, 0x400, 0);
    put32(&mut bytes, 0x404, u32::MAX);
    put64(&mut bytes, 0x540, 0x0000_7fff_1234_5678);
    dir(&mut bytes, 64, 11, 0xffff_ffff, 32);
    dir(&mut bytes, 64, 13, 0xffff_ffff, 32);
    put32(&mut bytes, 0x8c, 0xffff_ffff);
    put32(&mut bytes, 0x90, 10);
    let table = inspect(bytes).unwrap();
    assert!(
        table
            .entries
            .iter()
            .all(|symbol| symbol.kind != SymbolKind::Import)
    );
    for term in ["bound-only", "bound-import directory", "delay-load", "COFF"] {
        assert!(
            table.notes.iter().any(|note| note.contains(term)),
            "missing {term}"
        );
    }
}

#[test]
fn reserved_import_bits_and_disguised_bound_addresses_are_rejected() {
    for bits in [32, 64] {
        let mut bytes = fixture(bits);
        let flag = if bits == 64 { 1u64 << 63 } else { 1u64 << 31 };
        set_thunk(&mut bytes, bits, 0x500, flag | 0x1_0000);
        invalid(bytes, "reserved bits");
    }
    let mut bytes = fixture(64);
    put64(&mut bytes, 0x500, 0x0000_7fff_1234_5678);
    invalid(bytes, "reserved bits or bound address");
    let mut bytes = fixture(32);
    put32(&mut bytes, 0x400, 0);
    put32(&mut bytes, 0x540, 0x7000_0000);
    invalid(bytes, "unmapped");
}

#[test]
fn descriptors_lookup_tables_and_iats_require_bounded_terminators() {
    let mut bytes = fixture(32);
    dir(&mut bytes, 32, 1, 0x1200, 20);
    invalid(bytes, "unterminated PE import directory");
    let mut bytes = fixture(32);
    put32(&mut bytes, 0x548, 1);
    invalid(bytes, "IAT lacks its matching terminator");
    let mut bytes = fixture(32);
    put32(&mut bytes, 0x400, 0x17fc);
    put32(&mut bytes, 0x9fc, 0x8000_0007);
    invalid(bytes, "unmapped");
    let mut bytes = fixture(64);
    put32(&mut bytes, 0x400, 0x17fc);
    invalid(bytes, "no contiguous file bytes");
    let mut bytes = fixture(64);
    put32(&mut bytes, 0x410, 0);
    invalid(bytes, "missing PE import address table");
}

#[test]
fn export_counts_ordinals_and_rva_ranges_are_checked_before_allocation() {
    for field in [0x214, 0x218] {
        let mut bytes = fixture(64);
        put32(&mut bytes, field, MAX_SYMBOLS as u32 + 1);
        assert!(matches!(
            inspect(bytes),
            Err(FormatError::Limit {
                resource: "symbol table entries",
                ..
            })
        ));
    }
    let mut bytes = fixture(32);
    put16(&mut bytes, 0x270, 5);
    invalid(bytes, "ordinal is outside");
    let mut bytes = fixture(32);
    put32(&mut bytes, 0x210, u32::MAX);
    invalid(bytes, "ordinal overflow");
    let mut bytes = fixture(64);
    dir(&mut bytes, 64, 1, 0xffff_fff0, 40);
    invalid(bytes, "RVA span exceeds 32 bits");
    let mut bytes = fixture(32);
    put32(&mut bytes, 0x250, u32::MAX);
    invalid(bytes, "virtual address exceeds 32 bits");
}

#[test]
fn forwarders_and_names_cannot_read_past_their_own_ranges() {
    let mut bytes = fixture(32);
    put32(&mut bytes, 0x248, 0x117f);
    bytes[0x37f] = b'A';
    bytes[0x380] = 0;
    invalid(bytes, "unterminated symbol name");
    let mut bytes = fixture(32);
    put32(&mut bytes, 0x260, 0x17ff);
    bytes[0x9ff] = b'A';
    invalid(bytes, "unterminated symbol name");
}

#[test]
fn names_stop_before_an_interior_mapping_overlap() {
    for (value, valid) in [(b"A".as_slice(), true), (b"AB".as_slice(), false)] {
        let mut bytes = fixture(64);
        put32(&mut bytes, 0x260, 0x1400);
        text(&mut bytes, 0x600, value);
        let (mut document, mut image) = loaded(bytes);
        let mut overlap = image.regions.last().unwrap().clone();
        overlap.address = VirtualAddress::new(base(64) + 0x1402);
        overlap.memory_size = 2;
        overlap.file.offset = FileOffset::new(0x900);
        overlap.file.len = 2;
        image.regions.push(overlap);
        let result = super::super::inspect(&mut document, &image);
        if valid {
            assert_eq!(result.unwrap().entries[0].name, "A");
        } else {
            assert!(
                result
                    .unwrap_err()
                    .to_string()
                    .contains("unterminated symbol name")
            );
        }
    }
}

#[test]
fn name_budget_accepts_its_boundary_and_rejects_the_next_byte() {
    let mut bytes = fixture(64);
    resize_section(&mut bytes, 64, 0x1000);
    put32(&mut bytes, 0x260, 0x1400);
    bytes[0x600..0xa00].fill(b'A');
    bytes[0xa00] = 0;
    assert_eq!(inspect(bytes.clone()).unwrap().entries[0].name.len(), 1024);
    bytes[0xa00] = b'A';
    assert!(matches!(
        inspect(bytes),
        Err(FormatError::Limit {
            resource: "symbol name bytes",
            ..
        })
    ));
}

#[test]
fn unmapped_and_zero_fill_exports_preserve_addresses_without_fabricating_offsets() {
    let mut bytes = fixture(64);
    put32(&mut bytes, 0x250, 0x3000);
    let (mut document, image) = loaded(bytes);
    let table = super::super::inspect(&mut document, &image).unwrap();
    assert_eq!(
        table.entries[3].target,
        SymbolTarget::Virtual(VirtualAddress::new(base(64) + 0x3000))
    );
    assert_eq!(
        table.entries[3].target.file_offset(&image),
        Err(AddressError::Unmapped)
    );

    let mut bytes = fixture(64);
    put32(&mut bytes, OPTIONAL + 240 + 8, 0x900);
    put32(&mut bytes, 0x250, 0x1800);
    let (mut document, image) = loaded(bytes);
    let table = super::super::inspect(&mut document, &image).unwrap();
    assert!(table.entries[3].target.file_offset(&image).is_err());
}

#[test]
fn holes_are_skipped_but_a_named_empty_export_is_explicitly_unavailable() {
    let mut bytes = fixture(32);
    put32(&mut bytes, 0x250, 0);
    let table = inspect(bytes).unwrap();
    assert_eq!(table.entries[3].name, "Zulu");
    assert_eq!(table.entries[3].target, SymbolTarget::Unavailable);
    assert!(
        table
            .notes
            .iter()
            .any(|note| note.contains("empty address-table entry"))
    );
    assert!(
        !table
            .entries
            .iter()
            .any(|symbol| symbol.ordinal == Some(10))
    );
}

#[test]
fn records_remain_global_when_a_pe_is_inspected_in_a_nonzero_slice() {
    let (original, mut image) = loaded(fixture(64));
    let mut bytes = vec![0; 0x300];
    bytes.extend_from_slice(original.source().as_slice());
    image.slice.offset = FileOffset::new(0x300);
    for region in &mut image.regions {
        region.file.offset = FileOffset::new(region.file.offset.get() + 0x300);
        region.header.offset = FileOffset::new(region.header.offset.get() + 0x300);
    }
    let mut document = Document::new(MemorySource::new(bytes));
    let table = super::super::inspect(&mut document, &image).unwrap();
    assert_eq!(table.entries[0].record.offset, FileOffset::new(0x540));
    assert_eq!(table.entries[5].record.offset, FileOffset::new(0x800));
    assert_eq!(
        table.entries[5].target.file_offset(&image).unwrap(),
        FileOffset::new(0x840)
    );
}

#[test]
fn alias_expansion_cannot_exceed_the_retained_text_budget() {
    let mut bytes = fixture(64);
    resize_section(&mut bytes, 64, 0x10000);
    dir(&mut bytes, 64, 0, 0x1000, 0x100);
    dir(&mut bytes, 64, 1, 0, 0);
    put32(&mut bytes, 0x20c, 0);
    put32(&mut bytes, 0x214, 1);
    put32(&mut bytes, 0x218, 5000);
    put32(&mut bytes, 0x220, 0x2000);
    put32(&mut bytes, 0x224, 0x7000);
    for index in 0..5000 {
        put32(&mut bytes, 0x1200 + index * 4, 0xa000);
        put16(&mut bytes, 0x6200 + index * 2, 0);
    }
    bytes[0x9200..0x9600].fill(b'A');
    bytes[0x9600] = 0;
    assert!(matches!(
        inspect(bytes),
        Err(FormatError::Limit {
            resource: "symbol text bytes",
            ..
        })
    ));
}

#[test]
fn named_aliases_and_ordinal_only_exports_share_the_row_budget() {
    let mut bytes = fixture(64);
    resize_section(&mut bytes, 64, 0x20000);
    dir(&mut bytes, 64, 0, 0x1000, 0x100);
    dir(&mut bytes, 64, 1, 0, 0);
    put32(&mut bytes, 0x20c, 0);
    put32(&mut bytes, 0x214, 2);
    put32(&mut bytes, 0x218, MAX_SYMBOLS as u32);
    put32(&mut bytes, 0x220, 0x2000);
    put32(&mut bytes, 0x224, 0x12000);
    for index in 0..MAX_SYMBOLS {
        put32(&mut bytes, 0x1200 + index * 4, 0x1a000);
        put16(&mut bytes, 0x11200 + index * 2, 0);
    }
    text(&mut bytes, 0x19200, b"A");
    assert!(matches!(
        inspect(bytes),
        Err(FormatError::Limit {
            resource: "symbols",
            ..
        })
    ));
}

#[test]
fn repeated_bound_descriptors_cannot_exceed_the_read_budget() {
    let mut bytes = fixture(64);
    resize_section(&mut bytes, 64, 0x30000);
    bytes[0x200..].fill(0);
    dir(&mut bytes, 64, 0, 0, 0);
    dir(&mut bytes, 64, 1, 0x1000, 8193 * 20);
    for index in 0..8192 {
        let at = 0x200 + index * 20;
        put32(&mut bytes, at + 4, 1);
        put32(&mut bytes, at + 12, 0x30000);
    }
    bytes[0x2f200..0x2f600].fill(b'A');
    bytes[0x2f600] = 0;
    assert!(matches!(
        inspect(bytes),
        Err(FormatError::Limit {
            resource: "symbol read bytes",
            ..
        })
    ));
}
