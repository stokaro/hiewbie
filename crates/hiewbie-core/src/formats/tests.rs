//! All fixtures are generated from documented integer fields; no binary samples
//! or proprietary program material are imported.

use std::{cell::Cell, io, rc::Rc};

use super::*;
use crate::MemorySource;

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

fn pe_fixture(bits: u8) -> Vec<u8> {
    let endian = Endian::Little;
    let mut bytes = vec![0; 0x600];
    bytes[..2].copy_from_slice(b"MZ");
    put32(&mut bytes, 0x3c, 0x80, endian);
    bytes[0x80..0x84].copy_from_slice(b"PE\0\0");
    put16(
        &mut bytes,
        0x84,
        if bits == 64 { 0x8664 } else { 0x14c },
        endian,
    );
    put16(&mut bytes, 0x86, 1, endian);
    let size = if bits == 64 { 112 } else { 96 };
    put16(&mut bytes, 0x94, size, endian);
    let optional = 0x98;
    put16(
        &mut bytes,
        optional,
        if bits == 64 { 0x20b } else { 0x10b },
        endian,
    );
    put32(&mut bytes, optional + 16, 0x1000, endian);
    if bits == 64 {
        put64(&mut bytes, optional + 24, 0x1_4000_0000, endian);
    } else {
        put32(&mut bytes, optional + 28, 0x40_0000, endian);
    }
    put32(&mut bytes, optional + 32, 0x1000, endian);
    put32(&mut bytes, optional + 36, 0x200, endian);
    put32(&mut bytes, optional + 56, 0x3000, endian);
    put32(&mut bytes, optional + 60, 0x200, endian);
    let section = optional + usize::from(size);
    bytes[section..section + 5].copy_from_slice(b".text");
    put32(&mut bytes, section + 8, 0x300, endian);
    put32(&mut bytes, section + 12, 0x1000, endian);
    put32(&mut bytes, section + 16, 0x200, endian);
    put32(&mut bytes, section + 20, 0x200, endian);
    put32(&mut bytes, section + 36, 0x6000_0020, endian);
    bytes[0x200..0x203].copy_from_slice(&[0x90, 0xeb, 0xfd]);
    bytes
}

fn elf_fixture(bits: u8, endian: Endian) -> Vec<u8> {
    let mut bytes = vec![0; 0x300];
    bytes[..4].copy_from_slice(b"\x7fELF");
    bytes[4] = if bits == 64 { 2 } else { 1 };
    bytes[5] = if endian == Endian::Little { 1 } else { 2 };
    bytes[6] = 1;
    put16(&mut bytes, 16, 2, endian);
    put16(&mut bytes, 18, if bits == 64 { 183 } else { 40 }, endian);
    put32(&mut bytes, 20, 1, endian);
    let at = if bits == 64 { 64 } else { 52 };
    if bits == 64 {
        put64(&mut bytes, 24, 0x40_0100, endian);
        put64(&mut bytes, 32, at as u64, endian);
        put16(&mut bytes, 52, 64, endian);
        put16(&mut bytes, 54, 56, endian);
        put16(&mut bytes, 56, 1, endian);
        put32(&mut bytes, at, 1, endian);
        put32(&mut bytes, at + 4, 5, endian);
        put64(&mut bytes, at + 8, 0, endian);
        put64(&mut bytes, at + 16, 0x40_0000, endian);
        put64(&mut bytes, at + 32, 0x300, endian);
        put64(&mut bytes, at + 40, 0x400, endian);
        put64(&mut bytes, at + 48, 0x1000, endian);
    } else {
        put32(&mut bytes, 24, 0x40_0100, endian);
        put32(&mut bytes, 28, at as u32, endian);
        put16(&mut bytes, 40, 52, endian);
        put16(&mut bytes, 42, 32, endian);
        put16(&mut bytes, 44, 1, endian);
        put32(&mut bytes, at, 1, endian);
        put32(&mut bytes, at + 4, 0, endian);
        put32(&mut bytes, at + 8, 0x40_0000, endian);
        put32(&mut bytes, at + 16, 0x300, endian);
        put32(&mut bytes, at + 20, 0x400, endian);
        put32(&mut bytes, at + 24, 5, endian);
        put32(&mut bytes, at + 28, 0x1000, endian);
    }
    bytes
}

fn macho_fixture(bits: u8, endian: Endian, cpu: u32) -> Vec<u8> {
    let mut bytes = vec![0; 0x200];
    put32(
        &mut bytes,
        0,
        if bits == 64 { 0xfeed_facf } else { 0xfeed_face },
        endian,
    );
    put32(&mut bytes, 4, cpu, endian);
    put32(&mut bytes, 12, 2, endian);
    put32(&mut bytes, 16, 2, endian);
    let at = if bits == 64 { 32 } else { 28 };
    let segment_size = if bits == 64 { 72 } else { 56 };
    put32(&mut bytes, 20, segment_size as u32 + 24, endian);
    put32(&mut bytes, at, if bits == 64 { 0x19 } else { 1 }, endian);
    put32(&mut bytes, at + 4, segment_size as u32, endian);
    bytes[at + 8..at + 14].copy_from_slice(b"__TEXT");
    if bits == 64 {
        put64(&mut bytes, at + 24, 0x1_0000_0000, endian);
        put64(&mut bytes, at + 32, 0x1000, endian);
        put64(&mut bytes, at + 48, 0x200, endian);
        put32(&mut bytes, at + 56, 7, endian);
        put32(&mut bytes, at + 60, 5, endian);
    } else {
        put32(&mut bytes, at + 24, 0x1000, endian);
        put32(&mut bytes, at + 28, 0x1000, endian);
        put32(&mut bytes, at + 36, 0x200, endian);
        put32(&mut bytes, at + 40, 7, endian);
        put32(&mut bytes, at + 44, 5, endian);
    }
    let main = at + segment_size;
    put32(&mut bytes, main, 0x8000_0028, endian);
    put32(&mut bytes, main + 4, 24, endian);
    put64(&mut bytes, main + 8, 0x100, endian);
    bytes
}

fn fat_fixture(fat64: bool, endian: Endian) -> Vec<u8> {
    let mut bytes = vec![0; 0x600];
    put32(
        &mut bytes,
        0,
        if fat64 { 0xcafe_babf } else { 0xcafe_babe },
        endian,
    );
    put32(&mut bytes, 4, 2, endian);
    for (index, cpu) in [0x0100_0007, 0x0100_000c].into_iter().enumerate() {
        let at = 8 + index * if fat64 { 32 } else { 20 };
        let offset = 0x200 + index * 0x200;
        put32(&mut bytes, at, cpu, endian);
        if fat64 {
            put64(&mut bytes, at + 8, offset as u64, endian);
            put64(&mut bytes, at + 16, 0x200, endian);
            put32(&mut bytes, at + 24, 9, endian);
        } else {
            put32(&mut bytes, at + 8, offset as u32, endian);
            put32(&mut bytes, at + 12, 0x200, endian);
            put32(&mut bytes, at + 16, 9, endian);
        }
        bytes[offset..offset + 0x200].copy_from_slice(&macho_fixture(64, Endian::Little, cpu));
    }
    bytes
}

fn analyze(bytes: Vec<u8>) -> Result<Option<Executable>, FormatError> {
    inspect(&mut Document::new(MemorySource::new(bytes)), None)
}

#[test]
fn raw_detection_preserves_document_and_empty_input() {
    for bytes in [vec![], vec![0x90], b"ordinary data".to_vec()] {
        let mut document = Document::new(MemorySource::new(bytes.clone()));
        assert_eq!(inspect(&mut document, None).unwrap(), None);
        assert_eq!(document.source().as_slice(), bytes);
        assert_eq!(document.generation(), 0);
    }
}

#[test]
fn pe_widths_have_distinct_addresses_and_backing_fields() {
    for bits in [32, 64] {
        let image = analyze(pe_fixture(bits)).unwrap().unwrap();
        let base = if bits == 64 { 0x1_4000_0000 } else { 0x40_0000 };
        assert_eq!(image.bitness, bits);
        assert_eq!(image.entry, Some(VirtualAddress::new(base + 0x1000)));
        assert_eq!(
            image.file_to_va(FileOffset::new(0x201)),
            Ok(VirtualAddress::new(base + 0x1001))
        );
        assert_eq!(
            image.va_to_file(VirtualAddress::new(base + 0x1001)),
            Ok(FileOffset::new(0x201))
        );
        assert_eq!(
            image.file_to_va(FileOffset::new(0x10)),
            Ok(VirtualAddress::new(base + 0x10))
        );
        assert_eq!(image.rva_to_va(Rva::new(0x1000)), Ok(image.entry.unwrap()));
        assert_eq!(image.va_to_rva(image.entry.unwrap()), Ok(Rva::new(0x1000)));
        let field = image
            .fields
            .iter()
            .find(|field| field.name == "Optional.AddressOfEntryPoint")
            .unwrap();
        assert_eq!(
            field.range,
            FileRange {
                offset: FileOffset::new(0xa8),
                len: 4
            }
        );
        assert!(image.regions[1].executable);
        assert!(!image.regions[1].writable);
    }
}

#[test]
fn zero_fill_and_file_overlays_have_no_invented_mapping() {
    let image = analyze(pe_fixture(32)).unwrap().unwrap();
    assert_eq!(
        image.va_to_file(VirtualAddress::new(0x40_1250)),
        Err(AddressError::ZeroFill)
    );
    assert_eq!(
        image.file_to_va(FileOffset::new(0x500)),
        Err(AddressError::Unmapped)
    );
    assert_eq!(
        image.va_to_file(VirtualAddress::new(0x40_1300)),
        Err(AddressError::Unmapped)
    );
    assert_eq!(
        image.va_to_rva(VirtualAddress::new(0)),
        Err(AddressError::Unmapped)
    );
}

#[test]
fn pe_raw_alignment_padding_is_not_virtual_data() {
    let mut bytes = pe_fixture(32);
    put32(&mut bytes, 0xf8 + 8, 0x80, Endian::Little);
    let image = analyze(bytes).unwrap().unwrap();
    assert_eq!(
        image.file_to_va(FileOffset::new(0x27f)),
        Ok(VirtualAddress::new(0x40_107f))
    );
    assert_eq!(
        image.file_to_va(FileOffset::new(0x280)),
        Err(AddressError::Unmapped)
    );
    assert_eq!(
        image.va_to_file(VirtualAddress::new(0x40_1080)),
        Err(AddressError::Unmapped)
    );
}

#[test]
fn overlapping_pe_regions_are_ambiguous_in_both_domains() {
    let mut bytes = pe_fixture(32);
    put16(&mut bytes, 0x86, 2, Endian::Little);
    bytes.copy_within(0xf8..0xf8 + 40, 0xf8 + 40);
    let image = analyze(bytes).unwrap().unwrap();
    assert_eq!(
        image.file_to_va(FileOffset::new(0x200)),
        Err(AddressError::Ambiguous)
    );
    assert_eq!(
        image.va_to_file(VirtualAddress::new(0x40_1000)),
        Err(AddressError::Ambiguous)
    );
    assert_eq!(
        image.va_to_file(VirtualAddress::new(0x40_1250)),
        Err(AddressError::Ambiguous)
    );
}

#[test]
fn malformed_pe_ranges_directory_counts_and_machine_width_are_rejected() {
    let mut mutations = Vec::new();
    let mut bytes = pe_fixture(64);
    put64(&mut bytes, 0x98 + 24, u64::MAX - 0x100, Endian::Little);
    mutations.push(bytes);
    let mut bytes = pe_fixture(32);
    put32(&mut bytes, 0x98 + 92, 1, Endian::Little);
    mutations.push(bytes);
    let mut bytes = pe_fixture(32);
    put32(&mut bytes, 0xf8 + 20, 0xffff_ff00, Endian::Little);
    mutations.push(bytes);
    let mut bytes = pe_fixture(32);
    put16(&mut bytes, 0x84, 0x8664, Endian::Little);
    mutations.push(bytes);
    let mut bytes = pe_fixture(32);
    put32(&mut bytes, 0x98 + 28, 0xffff_f000, Endian::Little);
    mutations.push(bytes);
    for bytes in mutations {
        assert!(matches!(analyze(bytes), Err(FormatError::Invalid { .. })));
    }
}

#[test]
fn elf_widths_and_byte_orders_use_target_metadata() {
    for bits in [32, 64] {
        for endian in [Endian::Little, Endian::Big] {
            let image = analyze(elf_fixture(bits, endian)).unwrap().unwrap();
            assert_eq!(image.bitness, bits);
            assert_eq!(image.endian, endian);
            assert_eq!(
                image.machine,
                if bits == 64 {
                    Machine::Aarch64
                } else {
                    Machine::Arm
                }
            );
            assert_eq!(image.entry, Some(VirtualAddress::new(0x40_0100)));
            assert_eq!(
                image.va_to_file(image.entry.unwrap()),
                Ok(FileOffset::new(0x100))
            );
            assert_eq!(
                image.file_to_va(FileOffset::new(0x100)),
                image.entry.ok_or(AddressError::Unmapped)
            );
            assert_eq!(
                image.va_to_file(VirtualAddress::new(0x40_0300)),
                Err(AddressError::ZeroFill)
            );
            assert_eq!(image.rva_to_va(Rva::new(0)), Err(AddressError::NoImageBase));
        }
    }
}

#[test]
fn elf_alignment_virtual_overflow_and_table_extents_are_checked() {
    let mut mutations = Vec::new();
    let mut bytes = elf_fixture(64, Endian::Little);
    put64(&mut bytes, 64 + 48, 3, Endian::Little);
    mutations.push(bytes);
    let mut bytes = elf_fixture(64, Endian::Little);
    put64(&mut bytes, 64 + 16, u64::MAX - 0x100, Endian::Little);
    put64(&mut bytes, 64 + 48, 1, Endian::Little);
    mutations.push(bytes);
    let mut bytes = elf_fixture(32, Endian::Little);
    put32(&mut bytes, 52 + 8, 0xffff_ff00, Endian::Little);
    put32(&mut bytes, 52 + 28, 1, Endian::Little);
    mutations.push(bytes);
    let mut bytes = elf_fixture(64, Endian::Little);
    put64(&mut bytes, 32, u64::MAX - 8, Endian::Little);
    mutations.push(bytes);
    let mut bytes = elf_fixture(64, Endian::Little);
    put64(&mut bytes, 64 + 32, 0x500, Endian::Little);
    mutations.push(bytes);
    for bytes in mutations {
        assert!(matches!(analyze(bytes), Err(FormatError::Invalid { .. })));
    }
}

#[test]
fn elf_unknown_machine_stays_unknown_and_extended_sections_are_explicit() {
    let mut bytes = elf_fixture(64, Endian::Little);
    put16(&mut bytes, 18, 243, Endian::Little);
    let image = analyze(bytes.clone()).unwrap().unwrap();
    assert_eq!(image.machine, Machine::Other(243));
    put64(&mut bytes, 40, 0x200, Endian::Little);
    assert!(matches!(
        analyze(bytes),
        Err(FormatError::Unsupported("ELF extended section numbering"))
    ));
}

#[test]
fn elf_overlaps_preserve_inspection_and_reject_ambiguous_translation() {
    let mut bytes = elf_fixture(64, Endian::Little);
    put16(&mut bytes, 56, 2, Endian::Little);
    bytes.copy_within(64..120, 120);
    let image = analyze(bytes).unwrap().unwrap();
    assert_eq!(
        image.va_to_file(VirtualAddress::new(0x40_0100)),
        Err(AddressError::Ambiguous)
    );
}

#[test]
fn macho_widths_and_endianness_resolve_entry_and_zero_fill() {
    for bits in [32, 64] {
        for endian in [Endian::Little, Endian::Big] {
            let image = analyze(macho_fixture(
                bits,
                endian,
                if bits == 64 { 0x0100_000c } else { 7 },
            ))
            .unwrap()
            .unwrap();
            let base = if bits == 64 { 0x1_0000_0000 } else { 0x1000 };
            assert_eq!(image.endian, endian);
            assert_eq!(image.entry, Some(VirtualAddress::new(base + 0x100)));
            assert_eq!(
                image.va_to_file(image.entry.unwrap()),
                Ok(FileOffset::new(0x100))
            );
            assert_eq!(
                image.va_to_file(VirtualAddress::new(base + 0x200)),
                Err(AddressError::ZeroFill)
            );
        }
    }
}

#[test]
fn macho_command_bounds_counts_and_segment_ranges_are_checked() {
    let mut mutations = Vec::new();
    let mut bytes = macho_fixture(64, Endian::Little, 0x0100_000c);
    put32(&mut bytes, 36, 0, Endian::Little);
    mutations.push(bytes);
    let mut bytes = macho_fixture(64, Endian::Little, 0x0100_000c);
    put32(&mut bytes, 20, 88, Endian::Little);
    mutations.push(bytes);
    let mut bytes = macho_fixture(64, Endian::Little, 0x0100_000c);
    put32(&mut bytes, 32 + 64, 1, Endian::Little);
    mutations.push(bytes);
    let mut bytes = macho_fixture(64, Endian::Little, 0x0100_000c);
    put64(&mut bytes, 32 + 40, u64::MAX - 16, Endian::Little);
    mutations.push(bytes);
    let mut bytes = macho_fixture(64, Endian::Little, 0x0100_000c);
    put64(&mut bytes, 32 + 24, u64::MAX - 16, Endian::Little);
    mutations.push(bytes);
    for bytes in mutations {
        assert!(matches!(analyze(bytes), Err(FormatError::Invalid { .. })));
    }
}

#[test]
fn macho_unavailable_high_vm_is_explicit_and_old_thread_entry_is_not_invented() {
    let mut bytes = macho_fixture(64, Endian::Little, 0x0100_000c);
    put32(&mut bytes, 32 + 68, 1, Endian::Little);
    assert!(matches!(
        analyze(bytes),
        Err(FormatError::Unsupported("Mach-O high-VM core segments"))
    ));
    let mut bytes = macho_fixture(64, Endian::Little, 0x0100_000c);
    put32(&mut bytes, 104, 5, Endian::Little);
    assert_eq!(analyze(bytes).unwrap().unwrap().entry, None);
}

#[test]
fn fat_selection_is_explicit_host_independent_and_uses_global_offsets() {
    for fat64 in [false, true] {
        for endian in [Endian::Little, Endian::Big] {
            let mut document = Document::new(MemorySource::new(fat_fixture(fat64, endian)));
            let first = inspect(&mut document, None).unwrap().unwrap();
            assert_eq!(first.selected_slice, Some(0));
            assert_eq!(first.machine, Machine::X86_64);
            let second = inspect(&mut document, Some(1)).unwrap().unwrap();
            assert_eq!(second.selected_slice, Some(1));
            assert_eq!(second.machine, Machine::Aarch64);
            assert_eq!(second.slices.len(), 2);
            assert_eq!(second.slice.offset, FileOffset::new(0x400));
            assert_eq!(
                second.va_to_file(second.entry.unwrap()),
                Ok(FileOffset::new(0x500))
            );
            assert_eq!(
                second.slice_to_file(SliceOffset::new(0x100)),
                Ok(FileOffset::new(0x500))
            );
            assert_eq!(
                second.file_to_slice(FileOffset::new(0x500)),
                Ok(SliceOffset::new(0x100))
            );
            assert_eq!(
                second.file_to_slice(FileOffset::new(0x200)),
                Err(AddressError::Unmapped)
            );
            assert_eq!(
                second.slice_to_file(SliceOffset::new(0x200)),
                Err(AddressError::Unmapped)
            );
            let field = second
                .fields
                .iter()
                .find(|field| field.name == "Main.EntryOffset")
                .unwrap();
            assert_eq!(field.range.offset, FileOffset::new(0x400 + 112));
            assert!(matches!(
                inspect(&mut document, Some(2)),
                Err(FormatError::InvalidSlice {
                    requested: 2,
                    count: 2
                })
            ));
        }
    }
}

#[test]
fn fat_overlap_alignment_cpu_mismatch_and_slice_bounds_are_checked() {
    let mut mutations = Vec::new();
    let mut bytes = fat_fixture(true, Endian::Big);
    put64(&mut bytes, 40 + 8, 0x200, Endian::Big);
    mutations.push(bytes);
    let mut bytes = fat_fixture(true, Endian::Big);
    put32(&mut bytes, 8 + 24, 64, Endian::Big);
    mutations.push(bytes);
    let mut bytes = fat_fixture(true, Endian::Big);
    put64(&mut bytes, 8 + 16, u64::MAX, Endian::Big);
    mutations.push(bytes);
    let mut bytes = fat_fixture(true, Endian::Big);
    put32(&mut bytes, 8, 0x0100_000c, Endian::Big);
    mutations.push(bytes);
    for bytes in mutations {
        assert!(matches!(analyze(bytes), Err(FormatError::Invalid { .. })));
    }
}

#[test]
fn nested_fat_and_non_container_slice_requests_are_explicit_errors() {
    let mut bytes = fat_fixture(false, Endian::Big);
    bytes[0x200..0x204].copy_from_slice(&[0xca, 0xfe, 0xba, 0xbe]);
    assert!(matches!(analyze(bytes), Err(FormatError::Unsupported(_))));
    assert!(matches!(
        inspect(
            &mut Document::new(MemorySource::new(pe_fixture(32))),
            Some(0)
        ),
        Err(FormatError::InvalidSlice {
            requested: 0,
            count: 0
        })
    ));
}

#[test]
fn entry_counts_are_limited_before_allocating_or_iterating() {
    let mut pe = pe_fixture(32);
    put16(&mut pe, 0x86, u16::MAX, Endian::Little);
    let mut elf = elf_fixture(64, Endian::Little);
    put16(&mut elf, 56, u16::MAX, Endian::Little);
    let mut macho = macho_fixture(64, Endian::Little, 0x0100_000c);
    put32(&mut macho, 16, u32::MAX, Endian::Little);
    let mut fat = fat_fixture(true, Endian::Big);
    put32(&mut fat, 4, u32::MAX, Endian::Big);
    for bytes in [pe, elf, macho, fat] {
        assert!(matches!(analyze(bytes), Err(FormatError::Limit { .. })));
    }
}

#[test]
fn pending_header_changes_have_new_generation_and_discard_restores_metadata() {
    let mut document = Document::new(MemorySource::new(pe_fixture(32)));
    let before = inspect(&mut document, None).unwrap().unwrap();
    document
        .overwrite(FileOffset::new(0xa8), &0x1010_u32.to_le_bytes())
        .unwrap();
    let edited = inspect(&mut document, None).unwrap().unwrap();
    assert_ne!(before.generation, edited.generation);
    assert_eq!(edited.entry, Some(VirtualAddress::new(0x40_1010)));
    assert!(document.is_dirty());
    document.discard();
    let restored = inspect(&mut document, None).unwrap().unwrap();
    assert_eq!(restored.entry, before.entry);
    assert_ne!(restored.generation, before.generation);
}

struct SparseSource {
    header: Vec<u8>,
    len: u64,
    read_bytes: Rc<Cell<usize>>,
    largest_read: Rc<Cell<usize>>,
}

impl ByteSource for SparseSource {
    fn len(&self) -> u64 {
        self.len
    }
    fn read_at(&mut self, offset: FileOffset, out: &mut [u8]) -> io::Result<usize> {
        self.read_bytes.set(self.read_bytes.get() + out.len());
        self.largest_read
            .set(self.largest_read.get().max(out.len()));
        out.fill(0);
        if offset.get() < self.header.len() as u64 {
            let start = offset.get() as usize;
            let count = out.len().min(self.header.len() - start);
            out[..count].copy_from_slice(&self.header[start..start + count]);
        }
        Ok(out.len())
    }
}

#[test]
fn terabyte_sparse_source_reads_only_bounded_headers_and_maps_large_offsets() {
    let mut header = elf_fixture(64, Endian::Little);
    let offset = (1_u64 << 40) - 0x1000;
    put64(&mut header, 64 + 8, offset, Endian::Little);
    let read_bytes = Rc::new(Cell::new(0));
    let largest_read = Rc::new(Cell::new(0));
    let source = SparseSource {
        header,
        len: 1_u64 << 40,
        read_bytes: read_bytes.clone(),
        largest_read: largest_read.clone(),
    };
    let image = inspect(&mut Document::new(source), None).unwrap().unwrap();
    assert_eq!(
        image.va_to_file(image.entry.unwrap()),
        Ok(FileOffset::new(offset + 0x100))
    );
    assert!(read_bytes.get() < 1024);
    assert!(largest_read.get() <= 64);
}

#[test]
fn reader_enforces_total_budget_independently_of_read_size() {
    let mut document = Document::new(MemorySource::new(vec![0; 64]));
    let mut reader = Reader {
        document: &mut document,
        slice: FileRange {
            offset: FileOffset::new(0),
            len: 64,
        },
        read_bytes: MAX_FORMAT_READ_BYTES - 2,
    };
    assert!(matches!(
        reader.magic(),
        Err(FormatError::Limit {
            resource: "read bytes",
            ..
        })
    ));
}

#[test]
fn bounded_header_mutations_and_all_truncations_do_not_panic_or_mutate_bytes() {
    for fixture in [
        pe_fixture(64),
        elf_fixture(32, Endian::Big),
        macho_fixture(64, Endian::Little, 0x0100_000c),
        fat_fixture(true, Endian::Big),
    ] {
        for length in 0..fixture.len() {
            let _ = analyze(fixture[..length].to_vec());
        }
        let mut seed = 0x72a9_e113_u32;
        for _ in 0..512 {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let index = seed as usize % fixture.len();
            let mut bytes = fixture.clone();
            bytes[index] ^= (seed >> 24) as u8;
            let mut document = Document::new(MemorySource::new(bytes.clone()));
            let _ = inspect(&mut document, None);
            assert_eq!(document.source().as_slice(), bytes);
            assert_eq!(document.generation(), 0);
            assert!(!document.is_dirty());
        }
    }
}

#[test]
fn source_failures_propagate_without_synthesizing_format_bytes() {
    struct Failing;
    impl ByteSource for Failing {
        fn len(&self) -> u64 {
            4096
        }
        fn read_at(&mut self, _: FileOffset, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "fixture source failure",
            ))
        }
    }
    let mut document = Document::new(Failing);
    assert!(matches!(
        inspect(&mut document, None),
        Err(FormatError::Document(Error::Io(_)))
    ));
    assert_eq!(document.generation(), 0);
    assert!(!document.is_dirty());
}
