//! Original TE reader based on UEFI PI chapter 17 and the PE/COFF field layouts.
//!
//! TE and TE64 use the same 40-byte header. Width comes from Machine; ImageBase
//! and copied RVAs keep their original PE meaning. Only file pointers lose the
//! `StrippedSize - 40` prefix. No relocation or PE reconstruction is performed.

use super::{
    BinaryFormat, Endian, Executable, FileRange, FormatError, Machine, Reader, Region,
    StructureKind, StructureRecord, StructureStatus, VirtualAddress, image, name, u16_at, u32_at,
    u64_at,
};
use crate::ByteSource;

const HEADER_SIZE: u64 = 40;
const SECTION_SIZE: u64 = 40;
const DEBUG_SIZE: u64 = 28;

pub(super) fn parse<S: ByteSource>(
    reader: &mut Reader<'_, S>,
    generation: u64,
) -> Result<Executable, FormatError> {
    let header = reader.read::<40>(0)?;
    if &header[..2] != b"VZ" {
        return Err(reader.invalid("invalid TE signature", 0));
    }
    let raw_machine = u16_at(&header, 2, Endian::Little);
    let (machine, bitness) = match raw_machine {
        0x14c => (Machine::X86, 32),
        0x8664 => (Machine::X86_64, 64),
        // UEFI assigns 0x1c2 to ARMTHUMB_MIXED, permitting both instruction
        // states. The entry low bit selects its initial state, not the host.
        0x1c0 | 0x1c2 => (Machine::Arm, 32),
        0x1c4 => (Machine::Thumb, 32),
        0xaa64 => (Machine::Aarch64, 64),
        value => (Machine::Other(u32::from(value)), 0),
    };
    let format = match bitness {
        32 => BinaryFormat::Te32,
        64 => BinaryFormat::Te64,
        _ => BinaryFormat::TeUnknown,
    };
    let count = usize::from(header[4]);
    if count == 0 {
        return Err(reader.invalid("TE image has no sections", 4));
    }
    reader.table(HEADER_SIZE, count, SECTION_SIZE)?;
    let table_end = HEADER_SIZE + count as u64 * SECTION_SIZE;
    let stripped = u64::from(u16_at(&header, 6, Endian::Little));
    let bias = stripped
        .checked_sub(HEADER_SIZE)
        .ok_or_else(|| reader.invalid("TE stripped size is smaller than its header", 6))?;
    let base = u64_at(&header, 16, Endian::Little);
    let entry = u64::from(u32_at(&header, 8, Endian::Little));
    let code = u64::from(u32_at(&header, 12, Endian::Little));
    // The retained image header includes padding up to BaseOfCode, as in the
    // PI/EDK2 loader model. Keep only the padding left outside explicit section
    // mappings below; data/BSS preceding code must not gain a duplicate mapping.
    let headers_len = code
        .checked_sub(bias)
        .filter(|len| *len >= table_end)
        .ok_or_else(|| {
            reader.invalid("TE base of code does not contain the retained headers", 12)
        })?;
    reader.range(0, headers_len)?;
    let mut result = image(reader, generation, format, machine, Endian::Little, bitness);
    result.image_base = Some(VirtualAddress::new(base));
    checked_address(reader, &result, bias, headers_len, 16)?;
    if entry != 0 {
        if entry < bias {
            return Err(reader.invalid("TE entry RVA precedes the retained image", 8));
        }
        result.entry = Some(checked_address(reader, &result, entry, 1, 8)?);
    }
    checked_address(reader, &result, code, 1, 12)?;
    for (label, value, offset, len) in [
        ("Signature", 0x5a56, 0, 2),
        ("Machine", u64::from(raw_machine), 2, 2),
        ("NumberOfSections", count as u64, 4, 1),
        ("Subsystem", u64::from(header[5]), 5, 1),
        ("StrippedSize", stripped, 6, 2),
        ("AddressOfEntryPoint", entry, 8, 4),
        ("BaseOfCode", code, 12, 4),
        ("ImageBase", base, 16, 8),
    ] {
        reader.field(&mut result, format!("TE.{label}"), value, offset, len)?;
    }
    reader.note(
        &mut result,
        format!(
            "TE uses one 40-byte header; original file pointers subtract 0x{bias:X} \
             (StrippedSize - 40). VAs remain ImageBase + original RVA."
        ),
    )?;
    if bitness == 0 {
        reader.note(
            &mut result,
            "TE machine width is unknown; metadata remains available without assuming a decoder.",
        )?;
    }
    if raw_machine == 0x1c2 {
        reader.note(
            &mut result,
            "UEFI ARMTHUMB_MIXED permits ARM and Thumb code; the entry low bit selects its initial instruction state.",
        )?;
    }
    for index in 0..count {
        section(reader, &mut result, index, bias, table_end)?;
    }
    header_regions(reader, &mut result, bias, headers_len)?;
    for (index, label) in ["BaseRelocation", "Debug"].into_iter().enumerate() {
        let at = 24 + index as u64 * 8;
        let rva = u64::from(u32_at(&header, at as usize, Endian::Little));
        let size = u64::from(u32_at(&header, at as usize + 4, Endian::Little));
        reader.field(&mut result, format!("TE.{label}.RVA"), rva, at, 4)?;
        reader.field(&mut result, format!("TE.{label}.Size"), size, at + 4, 4)?;
        if size != 0 && rva == 0 {
            return Err(reader.invalid("TE directory has a size but no RVA", at));
        }
        let target = if size == 0 {
            None
        } else {
            Some(rva_range(reader, &result, rva, size, at)?)
        };
        let detail = if index == 0 && rva != 0 && size == 0 {
            reader.note(
                &mut result,
                "Position-independent TE image: relocation RVA is nonzero and its size is zero.",
            )?;
            "position-independent image; no relocation table".into()
        } else {
            format!("RVA 0x{rva:X}; size 0x{size:X}")
        };
        reader.record(
            &mut result,
            StructureRecord {
                name: format!("TE.{label}Directory"),
                kind: if index == 0 {
                    StructureKind::Relocation
                } else {
                    StructureKind::DebugDirectory
                },
                record: reader.range(at, 8)?,
                target,
                status: StructureStatus::Interpreted,
                detail,
            },
        )?;
        if let Some(target) = target {
            if index == 0 {
                relocations(reader, &mut result, target, rva, raw_machine)?;
            } else {
                debug_directory(reader, &mut result, target, bias)?;
            }
        }
    }
    Ok(result)
}

fn header_regions<S: ByteSource>(
    reader: &Reader<'_, S>,
    image: &mut Executable,
    bias: u64,
    len: u64,
) -> Result<(), FormatError> {
    let base = checked_address(reader, image, bias, len, 16)?.get();
    let section_count = image.regions.len();
    // At most two exclusions per u8-counted section. Exclude both spaces so a
    // section's disk padding/BSS or unusual file-to-VA displacement cannot be
    // reinterpreted as executable header bytes through the prefix mapping.
    let mut exclusions = Vec::with_capacity(section_count * 2);
    for region in &image.regions {
        for (start, size) in [
            (
                region.file.offset.get() - reader.slice.offset.get(),
                region.file.len,
            ),
            (
                region.address.get().saturating_sub(base),
                region.memory_size,
            ),
        ] {
            if start < len && size != 0 {
                exclusions.push((start, start.saturating_add(size).min(len)));
            }
        }
    }
    exclusions.sort_unstable();
    let mut cursor = 0;
    for (start, end) in exclusions.into_iter().chain([(len, len)]) {
        if cursor < start {
            let size = start - cursor;
            reader.region(
                image,
                Region {
                    name: if cursor == 0 {
                        "TE headers"
                    } else {
                        "TE header padding"
                    }
                    .into(),
                    file: reader.range(cursor, size)?,
                    address: VirtualAddress::new(base + cursor),
                    memory_size: size,
                    readable: true,
                    writable: false,
                    executable: false,
                    header: reader.range(0, HEADER_SIZE)?,
                },
            )?;
        }
        cursor = cursor.max(end);
    }
    let header_count = image.regions.len() - section_count;
    image.regions.rotate_right(header_count);
    Ok(())
}

fn checked_address<S: ByteSource>(
    reader: &Reader<'_, S>,
    image: &Executable,
    rva: u64,
    len: u64,
    at: u64,
) -> Result<VirtualAddress, FormatError> {
    if rva > u64::from(u32::MAX) || len > (1_u64 << 32) - rva {
        return Err(reader.invalid("TE RVA range exceeds 32 bits", at));
    }
    let base = image.image_base.map_or(0, VirtualAddress::get);
    let address = base
        .checked_add(rva)
        .ok_or_else(|| reader.invalid("TE virtual address overflow", at))?;
    let end = address
        .checked_add(len)
        .ok_or_else(|| reader.invalid("TE virtual range overflow", at))?;
    if image.bitness == 32 && (address > u64::from(u32::MAX) || end > 1_u64 << 32) {
        return Err(reader.invalid("TE32 virtual range exceeds 32 bits", at));
    }
    Ok(VirtualAddress::new(address))
}

fn section<S: ByteSource>(
    reader: &mut Reader<'_, S>,
    image: &mut Executable,
    index: usize,
    bias: u64,
    headers_len: u64,
) -> Result<(), FormatError> {
    let at = HEADER_SIZE + index as u64 * SECTION_SIZE;
    let bytes = reader.read::<40>(at)?;
    let label = name(&bytes[..8]);
    let virtual_size = u64::from(u32_at(&bytes, 8, Endian::Little));
    let rva = u64::from(u32_at(&bytes, 12, Endian::Little));
    let raw_size = u64::from(u32_at(&bytes, 16, Endian::Little));
    let pointer = u64::from(u32_at(&bytes, 20, Endian::Little));
    let flags = u32_at(&bytes, 36, Endian::Little);
    let address = checked_address(reader, image, rva, virtual_size, at + 12)?;
    if virtual_size != 0 && rva < bias + headers_len {
        return Err(reader.invalid("TE section overlaps the retained headers", at + 12));
    }
    let file = if raw_size == 0 {
        // Empty/BSS sections have no raw pointer to translate or navigate to.
        reader.range(0, 0)?
    } else {
        let offset = pointer.checked_sub(bias).ok_or_else(|| {
            reader.invalid("TE section raw pointer precedes stripped prefix", at + 20)
        })?;
        if offset < headers_len {
            return Err(reader.invalid("TE section data overlaps its headers", at + 20));
        }
        if raw_size > (1_u64 << 32) - pointer {
            return Err(reader.invalid("TE original file range exceeds 32 bits", at + 20));
        }
        reader.range(offset, raw_size)?
    };
    let record = reader.range(at, SECTION_SIZE)?;
    reader.region(
        image,
        Region {
            name: label.clone(),
            file,
            address,
            memory_size: virtual_size,
            readable: flags & 0x4000_0000 != 0,
            writable: flags & 0x8000_0000 != 0,
            executable: flags & 0x2000_0000 != 0,
            header: record,
        },
    )?;
    reader.record(
        image,
        StructureRecord {
            name: format!("Section[{index}] {label}"),
            kind: StructureKind::Section,
            record,
            target: (raw_size != 0).then_some(file),
            status: StructureStatus::Interpreted,
            detail: format!(
                "RVA 0x{rva:X}; virtual size 0x{virtual_size:X}; original raw pointer \
                 0x{pointer:X}; raw size 0x{raw_size:X}; characteristics 0x{flags:08X}"
            ),
        },
    )?;
    for (label, value, offset, len) in [
        ("VirtualSize", virtual_size, 8, 4),
        ("VirtualAddress", rva, 12, 4),
        ("SizeOfRawData", raw_size, 16, 4),
        ("PointerToRawData", pointer, 20, 4),
        (
            "PointerToRelocations",
            u64::from(u32_at(&bytes, 24, Endian::Little)),
            24,
            4,
        ),
        (
            "PointerToLinenumbers",
            u64::from(u32_at(&bytes, 28, Endian::Little)),
            28,
            4,
        ),
        (
            "NumberOfRelocations",
            u64::from(u16_at(&bytes, 32, Endian::Little)),
            32,
            2,
        ),
        (
            "NumberOfLinenumbers",
            u64::from(u16_at(&bytes, 34, Endian::Little)),
            34,
            2,
        ),
        ("Characteristics", u64::from(flags), 36, 4),
    ] {
        reader.field(
            image,
            format!("Section[{index}].{label}"),
            value,
            at + offset,
            len,
        )?;
    }
    Ok(())
}

/// A target must fit one unique mapping in its entirety, including any overlap
/// in the middle of the span. Looking up just the first/last byte is insufficient.
fn rva_range<S: ByteSource>(
    reader: &Reader<'_, S>,
    image: &Executable,
    rva: u64,
    len: u64,
    at: u64,
) -> Result<FileRange, FormatError> {
    let address = checked_address(reader, image, rva, len, at)?.get();
    let end = address + len;
    let mut target = None;
    for region in &image.regions {
        let start = region.address.get();
        let region_end = start + region.memory_size;
        if address >= region_end || start >= end {
            continue;
        }
        if target.is_some() || address < start || end > region_end {
            return Err(reader.invalid("TE RVA range is ambiguous or crosses mappings", at));
        }
        let delta = address - start;
        if delta >= region.file.len || len > region.file.len - delta {
            return Err(reader.invalid("TE RVA range includes non-file-backed memory", at));
        }
        let offset = region.file.offset.get() - reader.slice.offset.get() + delta;
        target = Some(reader.range(offset, len)?);
    }
    target.ok_or_else(|| reader.invalid("TE RVA range has no file-backed mapping", at))
}

fn relocations<S: ByteSource>(
    reader: &mut Reader<'_, S>,
    image: &mut Executable,
    range: FileRange,
    directory_rva: u64,
    raw_machine: u16,
) -> Result<(), FormatError> {
    let start = range.offset.get() - reader.slice.offset.get();
    let mut consumed = 0;
    let mut block_index = 0;
    while consumed < range.len {
        let at = start + consumed;
        if range.len - consumed < 8 || !(directory_rva + consumed).is_multiple_of(4) {
            return Err(reader.invalid("TE relocation block is truncated or unaligned", at));
        }
        let bytes = reader.read::<8>(at)?;
        let page = u64::from(u32_at(&bytes, 0, Endian::Little));
        let size = u64::from(u32_at(&bytes, 4, Endian::Little));
        if size < 8 || !size.is_multiple_of(2) || size > range.len - consumed {
            return Err(reader.invalid("invalid TE relocation block size", at + 4));
        }
        if !page.is_multiple_of(0x1000) {
            return Err(reader.invalid("TE relocation page RVA is not page aligned", at));
        }
        reader.table(at + 8, ((size - 8) / 2) as usize, 2)?;
        reader.field(
            image,
            format!("RelocationBlock[{block_index}].PageRVA"),
            page,
            at,
            4,
        )?;
        reader.field(
            image,
            format!("RelocationBlock[{block_index}].Size"),
            size,
            at + 4,
            4,
        )?;
        reader.record(
            image,
            StructureRecord {
                name: format!("RelocationBlock[{block_index}]"),
                kind: StructureKind::Relocation,
                record: reader.range(at, 8)?,
                target: if size == 8 {
                    None
                } else {
                    Some(reader.range(at + 8, size - 8)?)
                },
                status: StructureStatus::Interpreted,
                detail: format!("page RVA 0x{page:X}; block size 0x{size:X}"),
            },
        )?;
        let mut slot = 8;
        while slot < size {
            let entry_at = at + slot;
            let value = u16_at(&reader.read::<2>(entry_at)?, 0, Endian::Little);
            let kind = value >> 12;
            let offset = u64::from(value & 0xfff);
            let rva = page + offset;
            let (label, width) = relocation_type(raw_machine, kind);
            let entry_len = if kind == 4 { 4 } else { 2 };
            if entry_len > size - slot {
                return Err(
                    reader.invalid("TE HIGHADJ relocation is missing its second slot", entry_at)
                );
            }
            let mut detail = format!("type {kind}; {label}; target RVA 0x{rva:X}");
            let target = match width {
                Some(0) => None,
                Some(width) => Some(rva_range(reader, image, rva, width, entry_at)?),
                None => None,
            };
            if kind == 4 {
                let low = u16_at(&reader.read::<2>(entry_at + 2)?, 0, Endian::Little);
                detail.push_str(&format!("; low adjustment 0x{low:04X}"));
                reader.field(
                    image,
                    format!("Relocation[{block_index}:{}].Adjustment", (slot - 8) / 2),
                    u64::from(low),
                    entry_at + 2,
                    2,
                )?;
            }
            let entry_name = format!("Relocation[{block_index}:{}]", (slot - 8) / 2);
            reader.field(
                image,
                format!("{entry_name}.TypeOffset"),
                u64::from(value),
                entry_at,
                2,
            )?;
            reader.record(
                image,
                StructureRecord {
                    name: format!("{entry_name} {label}"),
                    kind: StructureKind::Relocation,
                    record: reader.range(entry_at, entry_len)?,
                    target,
                    status: if width.is_some() {
                        StructureStatus::Interpreted
                    } else {
                        StructureStatus::Unsupported
                    },
                    detail,
                },
            )?;
            slot += entry_len;
        }
        consumed += size;
        block_index += 1;
    }
    Ok(())
}

fn relocation_type(raw_machine: u16, kind: u16) -> (&'static str, Option<u64>) {
    match kind {
        0 => ("ABSOLUTE padding", Some(0)),
        1 => ("HIGH", Some(2)),
        2 => ("LOW", Some(2)),
        3 => ("HIGHLOW", Some(4)),
        4 => ("HIGHADJ", Some(2)),
        5 if matches!(raw_machine, 0x1c0 | 0x1c2 | 0x1c4) => ("ARM_MOV32", Some(8)),
        7 if matches!(raw_machine, 0x1c2 | 0x1c4) => ("THUMB_MOV32", Some(8)),
        10 => ("DIR64", Some(8)),
        _ => ("unsupported relocation type for this machine", None),
    }
}

fn debug_directory<S: ByteSource>(
    reader: &mut Reader<'_, S>,
    image: &mut Executable,
    range: FileRange,
    bias: u64,
) -> Result<(), FormatError> {
    let start = range.offset.get() - reader.slice.offset.get();
    if !range.len.is_multiple_of(DEBUG_SIZE) {
        return Err(reader.invalid("TE debug directory has a partial record", start));
    }
    let count = (range.len / DEBUG_SIZE) as usize;
    reader.table(start, count, DEBUG_SIZE)?;
    for index in 0..count {
        let at = start + index as u64 * DEBUG_SIZE;
        let bytes = reader.read::<28>(at)?;
        let kind = u32_at(&bytes, 12, Endian::Little);
        let size = u64::from(u32_at(&bytes, 16, Endian::Little));
        let rva = u64::from(u32_at(&bytes, 20, Endian::Little));
        let pointer = u64::from(u32_at(&bytes, 24, Endian::Little));
        for (label, value, offset, len) in [
            (
                "Characteristics",
                u64::from(u32_at(&bytes, 0, Endian::Little)),
                0,
                4,
            ),
            (
                "TimeDateStamp",
                u64::from(u32_at(&bytes, 4, Endian::Little)),
                4,
                4,
            ),
            (
                "MajorVersion",
                u64::from(u16_at(&bytes, 8, Endian::Little)),
                8,
                2,
            ),
            (
                "MinorVersion",
                u64::from(u16_at(&bytes, 10, Endian::Little)),
                10,
                2,
            ),
            ("Type", u64::from(kind), 12, 4),
            ("SizeOfData", size, 16, 4),
            ("AddressOfRawData", rva, 20, 4),
            ("PointerToRawData", pointer, 24, 4),
        ] {
            reader.field(
                image,
                format!("Debug[{index}].{label}"),
                value,
                at + offset,
                len,
            )?;
        }
        let target = debug_target(reader, image, rva, pointer, size, bias, at)?;
        reader.record(
            image,
            StructureRecord {
                name: format!("Debug[{index}] {}", debug_type(kind)),
                kind: StructureKind::DebugDirectory,
                record: reader.range(at, DEBUG_SIZE)?,
                target,
                status: if kind == 2 { StructureStatus::Interpreted } else { StructureStatus::Unsupported },
                detail: format!("type {kind}; RVA 0x{rva:X}; original raw pointer 0x{pointer:X}; size 0x{size:X}"),
            },
        )?;
        if let Some(payload) = target {
            debug_data(reader, image, index, kind, payload)?;
        }
    }
    Ok(())
}

fn debug_target<S: ByteSource>(
    reader: &Reader<'_, S>,
    image: &Executable,
    rva: u64,
    pointer: u64,
    size: u64,
    bias: u64,
    at: u64,
) -> Result<Option<FileRange>, FormatError> {
    if size == 0 {
        return Ok(None);
    }
    let raw = if pointer == 0 {
        None
    } else {
        let offset = pointer.checked_sub(bias).ok_or_else(|| {
            reader.invalid("TE debug raw pointer precedes stripped prefix", at + 24)
        })?;
        if size > (1_u64 << 32) - pointer {
            return Err(reader.invalid("TE debug original file range exceeds 32 bits", at + 24));
        }
        Some(reader.range(offset, size)?)
    };
    let mapped = if rva == 0 {
        None
    } else {
        Some(rva_range(reader, image, rva, size, at + 20)?)
    };
    match (raw, mapped) {
        (Some(raw), Some(mapped)) if raw != mapped => {
            Err(reader.invalid("TE debug RVA and raw pointer disagree", at + 20))
        }
        (Some(range), _) | (_, Some(range)) => Ok(Some(range)),
        (None, None) => Err(reader.invalid("TE debug payload has no location", at + 16)),
    }
}

fn debug_data<S: ByteSource>(
    reader: &mut Reader<'_, S>,
    image: &mut Executable,
    index: usize,
    kind: u32,
    payload: FileRange,
) -> Result<(), FormatError> {
    let at = payload.offset.get() - reader.slice.offset.get();
    let mut status = StructureStatus::Unsupported;
    let mut detail = format!(
        "{} payload; raw bytes available; payload interpretation unsupported",
        debug_type(kind)
    );
    if kind == 2 && payload.len >= 4 {
        let signature = reader.read::<4>(at)?;
        let (prefix_len, age_at) = match &signature {
            b"RSDS" => (24, 20),
            b"NB10" => (16, 12),
            _ => (0, 0),
        };
        reader.field(
            image,
            format!("Debug[{index}].CodeView.Signature"),
            u64::from(u32::from_le_bytes(signature)),
            at,
            4,
        )?;
        if prefix_len != 0 {
            if payload.len < prefix_len {
                return Err(reader.invalid("truncated TE CodeView header", at));
            }
            let age = u32_at(&reader.read::<4>(at + age_at)?, 0, Endian::Little);
            reader.field(
                image,
                format!("Debug[{index}].CodeView.Age"),
                u64::from(age),
                at + age_at,
                4,
            )?;
            detail = format!(
                "CodeView {}; age {age}; PDB filename and remaining payload retained as raw bytes",
                name(&signature)
            );
            if signature == *b"NB10" {
                let bytes = reader.read::<8>(at + 4)?;
                for (label, offset) in [("Offset", 0), ("TimeDateStamp", 4)] {
                    reader.field(
                        image,
                        format!("Debug[{index}].CodeView.{label}"),
                        u64::from(u32_at(&bytes, offset, Endian::Little)),
                        at + 4 + offset as u64,
                        4,
                    )?;
                }
            } else {
                // GUID is retained as four exact 32-bit words, avoiding a host-
                // endian UUID interpretation or loading an external PDB file.
                let bytes = reader.read::<16>(at + 4)?;
                for word in 0..4 {
                    reader.field(
                        image,
                        format!("Debug[{index}].CodeView.GuidWord[{word}]"),
                        u64::from(u32_at(&bytes, word * 4, Endian::Little)),
                        at + 4 + word as u64 * 4,
                        4,
                    )?;
                }
            }
            status = StructureStatus::Interpreted;
        }
    }
    reader.record(
        image,
        StructureRecord {
            name: format!("DebugData[{index}] {}", debug_type(kind)),
            kind: StructureKind::DebugData,
            record: payload,
            target: None,
            status,
            detail,
        },
    )
}

fn debug_type(kind: u32) -> &'static str {
    match kind {
        0 => "UNKNOWN",
        1 => "COFF",
        2 => "CodeView",
        3 => "FPO",
        4 => "MISC",
        6 => "FIXUP",
        9 => "BORLAND",
        12 => "VC_FEATURE",
        13 => "POGO",
        16 => "REPRO",
        20 => "EX_DLLCHARACTERISTICS",
        _ => "unknown debug type",
    }
}
