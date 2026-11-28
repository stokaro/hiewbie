//! Bounded load commands and section metadata from Apple's published layouts.

use super::super::{
    BinaryFormat, Endian, Executable, FileRange, FormatError, MAX_FORMAT_ENTRIES, Machine, Reader,
    Region, StructureKind, StructureRecord, StructureStatus, VirtualAddress, image, name, u32_at,
    u64_at,
};
use super::{machine, relocations, threads};
use crate::ByteSource;

pub(super) struct Section {
    pub name: String,
    pub address: u64,
    pub size: u64,
    pub file: Option<FileRange>,
    pub header: FileRange,
    pub relocations: u64,
    pub relocation_count: usize,
    zero_fill: bool,
}

pub(super) struct Symtab {
    pub offset: u64,
    pub count: usize,
    pub strings: u64,
    pub strings_size: u64,
    pub unavailable: Option<String>,
}

pub(super) struct DynamicRelocations {
    pub name: &'static str,
    pub offset: u64,
    pub count: usize,
}

struct Segment {
    name: String,
    address: u64,
    memory_size: u64,
    file: FileRange,
    protection: u32,
    flags: u32,
    header: FileRange,
}

impl Segment {
    fn backed_address(&self) -> u64 {
        self.address
            + if self.flags & 1 == 0 {
                0
            } else {
                self.memory_size - self.file.len
            }
    }
}

pub(in crate::formats) fn parse<S: ByteSource>(
    reader: &mut Reader<'_, S>,
    magic: [u8; 4],
    generation: u64,
) -> Result<Executable, FormatError> {
    let endian = if magic[0] == 0xfe {
        Endian::Big
    } else {
        Endian::Little
    };
    let bitness = if matches!(magic, [0xcf, 0xfa, 0xed, 0xfe] | [0xfe, 0xed, 0xfa, 0xcf]) {
        64
    } else {
        32
    };
    let header_size = if bitness == 64 { 32 } else { 28 };
    let header = reader.read::<28>(0)?;
    reader.range(0, header_size)?;
    let cpu = u32_at(&header, 4, endian);
    if matches!(
        (machine(cpu), bitness),
        (Machine::X86_64 | Machine::Aarch64, 32) | (Machine::X86 | Machine::Arm, 64)
    ) {
        return Err(reader.invalid("Mach-O CPU and header width disagree", 4));
    }
    let count = u32_at(&header, 16, endian) as usize;
    if count > MAX_FORMAT_ENTRIES {
        return Err(FormatError::Limit {
            resource: "load commands",
            limit: MAX_FORMAT_ENTRIES,
        });
    }
    let commands_size = u64::from(u32_at(&header, 20, endian));
    reader.range(header_size, commands_size)?;
    let commands_end = header_size + commands_size;
    let format = if bitness == 64 {
        BinaryFormat::MachO64
    } else {
        BinaryFormat::MachO32
    };
    let mut result = image(reader, generation, format, machine(cpu), endian, bitness);
    let file_type = u32_at(&header, 12, endian);
    for (label, value, offset) in [
        ("CpuType", cpu, 4),
        ("CpuSubtype", u32_at(&header, 8, endian), 8),
        ("FileType", file_type, 12),
        ("CommandCount", count as u32, 16),
        ("CommandBytes", commands_size as u32, 20),
        ("Flags", u32_at(&header, 24, endian), 24),
    ] {
        reader.field(
            &mut result,
            format!("MachO.{label}"),
            u64::from(value),
            offset,
            4,
        )?;
    }
    let mut sections = Vec::new();
    let mut segments = Vec::new();
    let mut symbols = None;
    let mut dynamic = Vec::new();
    let mut has_dysymtab = false;
    let mut main = None;
    let mut unix_entry = None;
    let mut has_unixthread = false;
    let mut thread_targets = Vec::new();
    let mut at = header_size;
    for index in 0..count {
        if commands_end - at < 8 {
            return Err(reader.invalid("Mach-O load-command header is truncated", at));
        }
        let command = reader.read::<8>(at)?;
        let kind = u32_at(&command, 0, endian);
        let size = u64::from(u32_at(&command, 4, endian));
        let alignment = if bitness == 64 { 8 } else { 4 };
        if size < 8 || size % alignment != 0 || size > commands_end - at {
            return Err(reader.invalid("Mach-O load command has invalid size or alignment", at + 4));
        }
        reader.field(
            &mut result,
            format!("Command[{index}].Type"),
            u64::from(kind),
            at,
            4,
        )?;
        reader.field(
            &mut result,
            format!("Command[{index}].Size"),
            size,
            at + 4,
            4,
        )?;
        let mut record = StructureRecord {
            name: format!("Command[{index}] 0x{kind:X}"),
            kind: StructureKind::LoadCommand,
            record: reader.range(at, size)?,
            target: None,
            status: StructureStatus::Interpreted,
            detail: String::new(),
        };
        match kind {
            1 | 0x19 => {
                let segment = segment(
                    reader,
                    &mut result,
                    at,
                    size,
                    index,
                    kind,
                    file_type,
                    &mut sections,
                )?;
                record.name = format!(
                    "Command[{index}] {}",
                    if bitness == 64 {
                        "LC_SEGMENT_64"
                    } else {
                        "LC_SEGMENT"
                    }
                );
                record.target = Some(segment.file);
                record.detail = format!(
                    "{} VA=0x{:X} memory=0x{:X} file=0x{:X}+0x{:X} protection=0x{:X} flags=0x{:X}{}",
                    segment.name,
                    segment.address,
                    segment.memory_size,
                    segment.file.offset.get(),
                    segment.file.len,
                    segment.protection,
                    segment.flags,
                    if segment.flags & 1 == 0 {
                        ""
                    } else {
                        "; SG_HIGHVM: zero-fill prefix, file-backed suffix"
                    },
                );
                segments.push(segment);
            }
            2 => {
                if size != 24 || symbols.is_some() {
                    return Err(
                        reader.invalid("Mach-O LC_SYMTAB is duplicated or has the wrong size", at)
                    );
                }
                let bytes = reader.read::<24>(at)?;
                let table = Symtab {
                    offset: u64::from(u32_at(&bytes, 8, endian)),
                    count: u32_at(&bytes, 12, endian) as usize,
                    strings: u64::from(u32_at(&bytes, 16, endian)),
                    strings_size: u64::from(u32_at(&bytes, 20, endian)),
                    unavailable: None,
                };
                let width = if bitness == 64 { 16 } else { 12 };
                let mut table = table;
                if let Err(error) = reader
                    .table(table.offset, table.count, width)
                    .and_then(|()| reader.range(table.strings, table.strings_size).map(|_| ()))
                {
                    table.unavailable = Some(error.to_string());
                }
                for (label, value, relative) in [
                    ("SymbolOffset", table.offset, 8),
                    ("SymbolCount", table.count as u64, 12),
                    ("StringOffset", table.strings, 16),
                    ("StringBytes", table.strings_size, 20),
                ] {
                    reader.field(
                        &mut result,
                        format!("Symtab.{label}"),
                        value,
                        at + relative,
                        4,
                    )?;
                }
                record.name = format!("Command[{index}] LC_SYMTAB");
                if let Some(error) = &table.unavailable {
                    record.status = StructureStatus::Unsupported;
                    record.detail = format!(
                        "nlist/string tables unavailable: {error}; descriptor remains raw; other image mappings are retained"
                    );
                } else {
                    record.target = Some(reader.range(table.offset, table.count as u64 * width)?);
                    record.detail = format!(
                        "{} bounded nlist records; string table 0x{:X}+0x{:X}; relocation references retain nlist provenance",
                        table.count,
                        reader.range(table.strings, 0)?.offset.get(),
                        table.strings_size
                    );
                }
                symbols = Some(table);
            }
            0xb => {
                if size != 80 || has_dysymtab {
                    return Err(reader
                        .invalid("Mach-O LC_DYSYMTAB is duplicated or has the wrong size", at));
                }
                has_dysymtab = true;
                let bytes = reader.read::<80>(at)?;
                for (word, label) in [
                    "LocalSymbolIndex",
                    "LocalSymbolCount",
                    "ExternalSymbolIndex",
                    "ExternalSymbolCount",
                    "UndefinedSymbolIndex",
                    "UndefinedSymbolCount",
                    "TocOffset",
                    "TocCount",
                    "ModuleOffset",
                    "ModuleCount",
                    "ExternalReferenceOffset",
                    "ExternalReferenceCount",
                    "IndirectSymbolOffset",
                    "IndirectSymbolCount",
                    "ExternalRelocationOffset",
                    "ExternalRelocationCount",
                    "LocalRelocationOffset",
                    "LocalRelocationCount",
                ]
                .into_iter()
                .enumerate()
                {
                    let relative = 8 + word * 4;
                    reader.field(
                        &mut result,
                        format!("Dysymtab.{label}"),
                        u64::from(u32_at(&bytes, relative, endian)),
                        at + relative as u64,
                        4,
                    )?;
                }
                for (label, relative) in [("External", 64), ("Local", 72)] {
                    let offset = u64::from(u32_at(&bytes, relative, endian));
                    let count = u32_at(&bytes, relative + 4, endian) as usize;
                    reader.table(offset, count, 8)?;
                    dynamic.push(DynamicRelocations {
                        name: label,
                        offset,
                        count,
                    });
                }
                record.name = format!("Command[{index}] LC_DYSYMTAB");
                record.status = StructureStatus::Unsupported;
                record.detail = "Descriptor fields and relocation records are decoded; loader-specific dynamic relocation bases, module/TOC/indirect tables are not interpreted".into();
            }
            4 | 5 => {
                if kind == 5 && has_unixthread {
                    return Err(reader.invalid("Mach-O LC_UNIXTHREAD is duplicated", at));
                }
                let thread = threads::parse(reader, &mut result, at, size, index)?;
                record.name = format!(
                    "Command[{index}] {}",
                    if kind == 5 {
                        "LC_UNIXTHREAD"
                    } else {
                        "LC_THREAD"
                    }
                );
                record.status = thread.status;
                record.detail = thread.detail;
                if let Some(entry) = &thread.entry {
                    thread_targets.push((result.records.len(), entry.address, entry.mode));
                }
                if kind == 5 {
                    has_unixthread = true;
                    unix_entry = thread.entry;
                } else {
                    record
                        .detail
                        .push_str("; thread context does not select the process entry point");
                }
            }
            0x8000_0028 => {
                if size != 24 || main.is_some() {
                    return Err(
                        reader.invalid("Mach-O LC_MAIN is duplicated or has the wrong size", at)
                    );
                }
                let bytes = reader.read::<24>(at)?;
                let offset = u64_at(&bytes, 8, endian);
                let stack = u64_at(&bytes, 16, endian);
                reader.field(&mut result, "Main.EntryOffset", offset, at + 8, 8)?;
                reader.field(&mut result, "Main.StackSize", stack, at + 16, 8)?;
                record.name = format!("Command[{index}] LC_MAIN");
                record.detail = format!("entry file offset=0x{offset:X}; stack size=0x{stack:X}");
                main = Some((offset, at + 8, result.records.len()));
            }
            _ => {
                record.status = StructureStatus::Unsupported;
                record.detail = if kind & 0x8000_0000 == 0 {
                    "Payload interpretation is unsupported; complete bounded raw command remains available".into()
                } else {
                    "Required-by-dyld command is not interpreted; raw inspection does not establish loadability".into()
                };
            }
        }
        reader.record(&mut result, record)?;
        at += size;
    }
    if at != commands_end {
        return Err(reader.invalid(
            "Mach-O command count does not consume the command table",
            at,
        ));
    }
    if file_type == 1 {
        for section in &sections {
            if section.file.is_some_and(|file| {
                file.len != 0 && file.offset.get() - reader.slice.offset.get() < commands_end
            }) {
                return Err(reader.invalid(
                    "Mach-O object section overlaps its header or load commands",
                    section.header.offset.get() - reader.slice.offset.get(),
                ));
            }
        }
    }
    if let Some((offset, field, record_index)) = main {
        let mut text = segments.iter().filter(|segment| segment.name == "__TEXT");
        let segment = text
            .next()
            .ok_or_else(|| reader.invalid("Mach-O LC_MAIN has no __TEXT mapping", field))?;
        if text.next().is_some() {
            return Err(reader.invalid("Mach-O LC_MAIN has ambiguous __TEXT mappings", field));
        }
        if segment.file.offset != reader.slice.offset {
            return Err(FormatError::Unsupported(
                "Mach-O LC_MAIN with nonzero __TEXT file offset",
            ));
        }
        let address = segment
            .backed_address()
            .checked_add(offset)
            .ok_or_else(|| reader.invalid("Mach-O entry address overflow", field))?;
        let file = result
            .va_to_file(VirtualAddress::new(address))
            .map_err(|_| {
                reader.invalid("Mach-O LC_MAIN is unmapped, zero-fill, or ambiguous", field)
            })?;
        result.entry = Some(VirtualAddress::new(address));
        result.records[record_index].target = Some(FileRange {
            offset: file,
            len: 1,
        });
        if unix_entry.is_some() {
            reader.note(&mut result, "LC_MAIN selects the process entry; LC_UNIXTHREAD remains available as thread-state metadata.")?;
        }
    } else if let Some(entry) = unix_entry {
        result.entry = Some(VirtualAddress::new(entry.address));
        result.entry_mode = entry.mode;
        if let Err(error) = result.va_to_file(VirtualAddress::new(entry.address)) {
            reader.note(
                &mut result,
                format!(
                    "LC_UNIXTHREAD PC at file 0x{:X} cannot be followed: {error}.",
                    reader.range(entry.field, 0)?.offset.get()
                ),
            )?;
        }
    }
    for (index, address, mode) in thread_targets {
        if threads::aligned(address, mode)
            && let Ok(offset) = result.va_to_file(VirtualAddress::new(address))
        {
            result.records[index].target = Some(FileRange { offset, len: 1 });
        }
    }
    relocations::parse(reader, &mut result, &sections, symbols.as_ref(), &dynamic)?;
    Ok(result)
}

#[allow(clippy::too_many_arguments)]
fn segment<S: ByteSource>(
    reader: &mut Reader<'_, S>,
    image: &mut Executable,
    at: u64,
    size: u64,
    index: usize,
    kind: u32,
    file_type: u32,
    sections: &mut Vec<Section>,
) -> Result<Segment, FormatError> {
    let wide = image.bitness == 64;
    if (kind == 0x19) != wide {
        return Err(reader.invalid("Mach-O segment and header widths disagree", at));
    }
    let segment_size = if wide { 72 } else { 56 };
    let section_size = if wide { 80 } else { 68 };
    if size < segment_size {
        return Err(reader.invalid("Mach-O segment command is too short", at));
    }
    let mut bytes = [0; 72];
    if wide {
        bytes = reader.read::<72>(at)?;
    } else {
        bytes[..56].copy_from_slice(&reader.read::<56>(at)?);
    }
    let e = image.endian;
    let (address, memory_size, offset, file_size, protection, count, flags) = if wide {
        (
            u64_at(&bytes, 24, e),
            u64_at(&bytes, 32, e),
            u64_at(&bytes, 40, e),
            u64_at(&bytes, 48, e),
            u32_at(&bytes, 60, e),
            u32_at(&bytes, 64, e),
            u32_at(&bytes, 68, e),
        )
    } else {
        (
            u64::from(u32_at(&bytes, 24, e)),
            u64::from(u32_at(&bytes, 28, e)),
            u64::from(u32_at(&bytes, 32, e)),
            u64::from(u32_at(&bytes, 36, e)),
            u32_at(&bytes, 44, e),
            u32_at(&bytes, 48, e),
            u32_at(&bytes, 52, e),
        )
    };
    if sections
        .len()
        .checked_add(count as usize)
        .is_none_or(|count| count > MAX_FORMAT_ENTRIES)
    {
        return Err(FormatError::Limit {
            resource: "Mach-O sections",
            limit: MAX_FORMAT_ENTRIES,
        });
    }
    if size != segment_size + u64::from(count) * section_size {
        return Err(reader.invalid("Mach-O section count does not match command size", at));
    }
    if file_size > memory_size {
        return Err(reader.invalid("Mach-O segment has more file bytes than memory", at));
    }
    let end = address
        .checked_add(memory_size)
        .ok_or_else(|| reader.invalid("Mach-O virtual range overflow", at))?;
    if !wide && end > 1_u64 << 32 {
        return Err(reader.invalid("Mach-O32 virtual range exceeds 32 bits", at));
    }
    let segment = Segment {
        name: name(&bytes[8..24]),
        address,
        memory_size,
        file: reader.range(offset, file_size)?,
        protection,
        flags,
        header: reader.range(at, size)?,
    };
    let scalar = if wide { 8 } else { 4 };
    for (label, value, relative) in [
        ("VirtualAddress", address, 24),
        ("MemorySize", memory_size, if wide { 32 } else { 28 }),
        ("FileOffset", offset, if wide { 40 } else { 32 }),
        ("FileSize", file_size, if wide { 48 } else { 36 }),
    ] {
        reader.field(
            image,
            format!("Segment[{index}].{label}"),
            value,
            at + relative,
            scalar,
        )?;
    }
    for (label, relative) in [
        ("MaxProtection", if wide { 56 } else { 40 }),
        ("InitialProtection", if wide { 60 } else { 44 }),
        ("SectionCount", if wide { 64 } else { 48 }),
        ("Flags", if wide { 68 } else { 52 }),
    ] {
        reader.field(
            image,
            format!("Segment[{index}].{label}"),
            u64::from(u32_at(&bytes, relative, e)),
            at + relative as u64,
            4,
        )?;
    }
    let backed = segment.backed_address();
    if (file_type != 1 || count == 0) && backed > address {
        reader.region(
            image,
            Region {
                name: format!("{} (zero-fill prefix)", segment.name),
                file: reader.range(offset, 0)?,
                address: VirtualAddress::new(address),
                memory_size: backed - address,
                readable: protection & 1 != 0,
                writable: protection & 2 != 0,
                executable: protection & 4 != 0,
                header: segment.header,
            },
        )?;
    }
    if file_type != 1 || count == 0 {
        reader.region(
            image,
            Region {
                name: segment.name.clone(),
                file: segment.file,
                address: VirtualAddress::new(backed),
                memory_size: if flags & 1 != 0 {
                    file_size
                } else {
                    memory_size
                },
                readable: protection & 1 != 0,
                writable: protection & 2 != 0,
                executable: protection & 4 != 0,
                header: segment.header,
            },
        )?;
    }
    for section_index in 0..count {
        let cursor = at + segment_size + u64::from(section_index) * section_size;
        let section = section(
            reader,
            image,
            &segment,
            cursor,
            file_type,
            sections.len() + 1,
        )?;
        // Relocatable objects retain section identity. Their compact on-disk
        // layout need not share the segment's file-to-VM displacement.
        if file_type == 1 && (section.file.is_some() || section.zero_fill) {
            reader.region(
                image,
                Region {
                    name: section.name.clone(),
                    file: section.file.unwrap_or(reader.range(0, 0)?),
                    address: VirtualAddress::new(section.address),
                    memory_size: section.size,
                    readable: protection & 1 != 0,
                    writable: protection & 2 != 0,
                    executable: protection & 4 != 0,
                    header: section.header,
                },
            )?;
        }
        sections.push(section);
    }
    Ok(segment)
}

fn section<S: ByteSource>(
    reader: &mut Reader<'_, S>,
    image: &mut Executable,
    segment: &Segment,
    at: u64,
    file_type: u32,
    ordinal: usize,
) -> Result<Section, FormatError> {
    let wide = image.bitness == 64;
    let width = if wide { 80 } else { 68 };
    let mut bytes = [0; 80];
    if wide {
        bytes = reader.read::<80>(at)?;
    } else {
        bytes[..68].copy_from_slice(&reader.read::<68>(at)?);
    }
    let e = image.endian;
    let (address, size, tail) = if wide {
        (u64_at(&bytes, 32, e), u64_at(&bytes, 40, e), 48)
    } else {
        (
            u64::from(u32_at(&bytes, 32, e)),
            u64::from(u32_at(&bytes, 36, e)),
            40,
        )
    };
    let offset = u64::from(u32_at(&bytes, tail, e));
    let alignment = u32_at(&bytes, tail + 4, e);
    let relocations = u64::from(u32_at(&bytes, tail + 8, e));
    let relocation_count = u32_at(&bytes, tail + 12, e) as usize;
    let flags = u32_at(&bytes, tail + 16, e);
    let section_type = flags & 0xff;
    let end = address
        .checked_add(size)
        .ok_or_else(|| reader.invalid("Mach-O section virtual range overflow", at + 32))?;
    if !wide && end > 1_u64 << 32 {
        return Err(reader.invalid("Mach-O32 section virtual range exceeds 32 bits", at + 32));
    }
    if size != 0 && (address < segment.address || end > segment.address + segment.memory_size) {
        return Err(reader.invalid(
            "Mach-O section is outside its segment's virtual range",
            at + 32,
        ));
    }
    reader.table(relocations, relocation_count, 8)?;
    let zero_fill = matches!(section_type, 1 | 0xc | 0x12);
    let type_name = section_type_name(section_type);
    let mut mapping_disagreement = false;
    let file = if zero_fill || type_name.is_none() {
        None
    } else {
        let range = reader.range(offset, size)?;
        if size != 0 {
            let delta = range
                .offset
                .get()
                .checked_sub(segment.file.offset.get())
                .ok_or_else(|| {
                    reader.invalid(
                        "Mach-O section precedes its segment's file bytes",
                        at + tail as u64,
                    )
                })?;
            if delta > segment.file.len || size > segment.file.len - delta {
                return Err(reader.invalid(
                    "Mach-O section exceeds its segment's file bytes",
                    at + tail as u64,
                ));
            }
            if file_type != 1 && segment.backed_address().checked_add(delta) != Some(address) {
                mapping_disagreement = true;
            }
        }
        if mapping_disagreement {
            None
        } else {
            Some(range)
        }
    };
    let section = Section {
        name: format!("{},{}", name(&bytes[16..32]), name(&bytes[..16])),
        address,
        size,
        file,
        header: reader.range(at, width)?,
        relocations,
        relocation_count,
        zero_fill,
    };
    reader.field(
        image,
        format!("Section[{ordinal}].Address"),
        address,
        at + 32,
        if wide { 8 } else { 4 },
    )?;
    reader.field(
        image,
        format!("Section[{ordinal}].Size"),
        size,
        at + if wide { 40 } else { 36 },
        if wide { 8 } else { 4 },
    )?;
    for (word, label) in [
        "FileOffset",
        "Alignment",
        "RelocationOffset",
        "RelocationCount",
        "Flags",
        "Reserved1",
        "Reserved2",
        "Reserved3",
    ]
    .into_iter()
    .take(if wide { 8 } else { 7 })
    .enumerate()
    {
        let relative = tail + word * 4;
        reader.field(
            image,
            format!("Section[{ordinal}].{label}"),
            u64::from(u32_at(&bytes, relative, e)),
            at + relative as u64,
            4,
        )?;
    }
    reader.record(image, StructureRecord {
        name: format!("Section[{ordinal}] {}", section.name), kind: StructureKind::Section,
        record: section.header, target: file,
        status: if type_name.is_some() && !mapping_disagreement { StructureStatus::Interpreted } else { StructureStatus::Unsupported },
        detail: format!("{}; flags=0x{flags:08X}; VA=0x{address:X}; size=0x{size:X}; alignment=2^{alignment}; relocations={relocation_count}{}{}{}",
            type_name.unwrap_or("unknown section type; backing semantics unsupported"),
            if zero_fill { "; zero-fill: no file bytes" } else { "" },
            if flags & 0x8000_0400 != 0 { "; instructions" } else { "" },
            if mapping_disagreement { "; section file and virtual mappings disagree; no section target" } else { "" }),
    })?;
    Ok(section)
}

fn section_type_name(kind: u32) -> Option<&'static str> {
    [
        "S_REGULAR",
        "S_ZEROFILL",
        "S_CSTRING_LITERALS",
        "S_4BYTE_LITERALS",
        "S_8BYTE_LITERALS",
        "S_LITERAL_POINTERS",
        "S_NON_LAZY_SYMBOL_POINTERS",
        "S_LAZY_SYMBOL_POINTERS",
        "S_SYMBOL_STUBS",
        "S_MOD_INIT_FUNC_POINTERS",
        "S_MOD_TERM_FUNC_POINTERS",
        "S_COALESCED",
        "S_GB_ZEROFILL",
        "S_INTERPOSING",
        "S_16BYTE_LITERALS",
        "S_DTRACE_DOF",
        "S_LAZY_DYLIB_SYMBOL_POINTERS",
        "S_THREAD_LOCAL_REGULAR",
        "S_THREAD_LOCAL_ZEROFILL",
        "S_THREAD_LOCAL_VARIABLES",
        "S_THREAD_LOCAL_VARIABLE_POINTERS",
        "S_THREAD_LOCAL_INIT_FUNCTION_POINTERS",
        "S_INIT_FUNC_OFFSETS",
    ]
    .get(kind as usize)
    .copied()
}
