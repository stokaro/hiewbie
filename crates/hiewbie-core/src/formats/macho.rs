use super::{
    BinaryFormat, Endian, Executable, FatSlice, FormatError, HeaderField, MAX_FAT_SLICES,
    MAX_FORMAT_ENTRIES, Machine, Reader, Region, VirtualAddress, image, name, parse_image, u32_at,
    u64_at,
};
use crate::ByteSource;

pub(super) fn is_thin(magic: [u8; 4]) -> bool {
    matches!(
        magic,
        [0xce, 0xfa, 0xed, 0xfe]
            | [0xcf, 0xfa, 0xed, 0xfe]
            | [0xfe, 0xed, 0xfa, 0xce]
            | [0xfe, 0xed, 0xfa, 0xcf]
    )
}

pub(super) fn is_fat(magic: [u8; 4]) -> bool {
    matches!(
        magic,
        [0xca, 0xfe, 0xba, 0xbe]
            | [0xbe, 0xba, 0xfe, 0xca]
            | [0xca, 0xfe, 0xba, 0xbf]
            | [0xbf, 0xba, 0xfe, 0xca]
    )
}

fn machine(cpu: u32) -> Machine {
    match cpu {
        7 => Machine::X86,
        0x0100_0007 => Machine::X86_64,
        12 => Machine::Arm,
        0x0100_000c => Machine::Aarch64,
        value => Machine::Other(value),
    }
}

// Segment and entry-point structures follow the published Apple Mach-O headers.
// Old thread-state entries, high-VM core mappings and symbols remain explicit gaps.
pub(super) fn parse<S: ByteSource>(
    reader: &mut Reader<'_, S>,
    magic: [u8; 4],
    generation: u64,
) -> Result<Executable, FormatError> {
    let endian = if magic[0] == 0xfe {
        Endian::Big
    } else {
        Endian::Little
    };
    let bitness = if magic == [0xcf, 0xfa, 0xed, 0xfe] || magic == [0xfe, 0xed, 0xfa, 0xcf] {
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
    let count = usize::try_from(u32_at(&header, 16, endian)).map_err(|_| FormatError::Limit {
        resource: "load commands",
        limit: MAX_FORMAT_ENTRIES,
    })?;
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
    reader.field(&mut result, "MachO.CpuType", u64::from(cpu), 4, 4)?;
    reader.field(
        &mut result,
        "MachO.CpuSubtype",
        u64::from(u32_at(&header, 8, endian)),
        8,
        4,
    )?;
    reader.field(
        &mut result,
        "MachO.FileType",
        u64::from(u32_at(&header, 12, endian)),
        12,
        4,
    )?;
    reader.field(&mut result, "MachO.CommandCount", count as u64, 16, 4)?;
    reader.field(&mut result, "MachO.CommandBytes", commands_size, 20, 4)?;
    reader.field(
        &mut result,
        "MachO.Flags",
        u64::from(u32_at(&header, 24, endian)),
        24,
        4,
    )?;
    let mut at = header_size;
    let mut entry = None;
    let mut total_sections = 0_usize;
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
        match kind {
            1 | 0x19 => {
                if (kind == 0x19) != (bitness == 64) {
                    return Err(reader.invalid("Mach-O segment and header widths disagree", at));
                }
                let segment_size = if bitness == 64 { 72 } else { 56 };
                let section_size = if bitness == 64 { 80 } else { 68 };
                if size < segment_size {
                    return Err(reader.invalid("Mach-O segment command is too short", at));
                }
                let mut segment = [0; 72];
                if bitness == 64 {
                    segment = reader.read::<72>(at)?;
                } else {
                    segment[..56].copy_from_slice(&reader.read::<56>(at)?);
                }
                let (address, memory_size, offset, file_size, protection, sections, flags) =
                    if bitness == 64 {
                        (
                            u64_at(&segment, 24, endian),
                            u64_at(&segment, 32, endian),
                            u64_at(&segment, 40, endian),
                            u64_at(&segment, 48, endian),
                            u32_at(&segment, 60, endian),
                            u32_at(&segment, 64, endian),
                            u32_at(&segment, 68, endian),
                        )
                    } else {
                        (
                            u64::from(u32_at(&segment, 24, endian)),
                            u64::from(u32_at(&segment, 28, endian)),
                            u64::from(u32_at(&segment, 32, endian)),
                            u64::from(u32_at(&segment, 36, endian)),
                            u32_at(&segment, 44, endian),
                            u32_at(&segment, 48, endian),
                            u32_at(&segment, 52, endian),
                        )
                    };
                total_sections = total_sections
                    .checked_add(sections as usize)
                    .filter(|count| *count <= MAX_FORMAT_ENTRIES)
                    .ok_or(FormatError::Limit {
                        resource: "Mach-O sections",
                        limit: MAX_FORMAT_ENTRIES,
                    })?;
                if size != segment_size + u64::from(sections) * section_size {
                    return Err(
                        reader.invalid("Mach-O section count does not match command size", at)
                    );
                }
                if file_size > memory_size {
                    return Err(
                        reader.invalid("Mach-O segment has more file bytes than memory", at)
                    );
                }
                if bitness == 32 && memory_size > (1_u64 << 32) - address {
                    return Err(reader.invalid("Mach-O32 virtual range exceeds 32 bits", at));
                }
                if flags & 1 != 0 {
                    return Err(FormatError::Unsupported("Mach-O high-VM core segments"));
                }
                reader.region(
                    &mut result,
                    Region {
                        name: name(&segment[8..24]),
                        file: reader.range(offset, file_size)?,
                        address: VirtualAddress::new(address),
                        memory_size,
                        readable: protection & 1 != 0,
                        writable: protection & 2 != 0,
                        executable: protection & 4 != 0,
                        header: reader.range(at, size)?,
                    },
                )?;
                let scalar_size = u64::from(bitness / 8);
                for (label, value, relative) in [
                    ("VirtualAddress", address, 24),
                    (
                        "MemorySize",
                        memory_size,
                        if bitness == 64 { 32 } else { 28 },
                    ),
                    ("FileOffset", offset, if bitness == 64 { 40 } else { 32 }),
                    ("FileSize", file_size, if bitness == 64 { 48 } else { 36 }),
                ] {
                    reader.field(
                        &mut result,
                        format!("Segment[{index}].{label}"),
                        value,
                        at + relative,
                        scalar_size,
                    )?;
                }
            }
            0x8000_0028 => {
                if size != 24 || entry.is_some() {
                    return Err(
                        reader.invalid("Mach-O LC_MAIN is duplicated or has the wrong size", at)
                    );
                }
                let bytes = reader.read::<24>(at)?;
                let offset = u64_at(&bytes, 8, endian);
                reader.field(&mut result, "Main.EntryOffset", offset, at + 8, 8)?;
                entry = Some((offset, at + 8));
            }
            _ => {}
        }
        at += size;
    }
    if at != commands_end {
        return Err(reader.invalid(
            "Mach-O command count does not consume the command table",
            at,
        ));
    }
    if let Some((offset, field)) = entry {
        let mut text = result
            .regions
            .iter()
            .filter(|region| region.name == "__TEXT");
        let region = text
            .next()
            .ok_or_else(|| reader.invalid("Mach-O LC_MAIN has no __TEXT mapping", field))?;
        if text.next().is_some() {
            return Err(reader.invalid("Mach-O LC_MAIN has ambiguous __TEXT mappings", field));
        }
        if region.file.offset != reader.slice.offset {
            return Err(FormatError::Unsupported(
                "Mach-O LC_MAIN with nonzero __TEXT file offset",
            ));
        }
        let address = region
            .address
            .get()
            .checked_add(offset)
            .ok_or_else(|| reader.invalid("Mach-O entry address overflow", field))?;
        result
            .va_to_file(VirtualAddress::new(address))
            .map_err(|_| {
                reader.invalid("Mach-O LC_MAIN is unmapped, zero-fill, or ambiguous", field)
            })?;
        result.entry = Some(VirtualAddress::new(address));
    }
    Ok(result)
}

pub(super) fn parse_fat<S: ByteSource>(
    reader: &mut Reader<'_, S>,
    requested: Option<usize>,
    generation: u64,
) -> Result<Executable, FormatError> {
    let header = reader.read::<8>(0)?;
    let endian = if header[0] == 0xca {
        Endian::Big
    } else {
        Endian::Little
    };
    let fat64 = header[..4] == [0xca, 0xfe, 0xba, 0xbf] || header[..4] == [0xbf, 0xba, 0xfe, 0xca];
    let count = u32_at(&header, 4, endian) as usize;
    if count == 0 {
        return Err(reader.invalid("universal Mach-O contains no slices", 4));
    }
    if count > MAX_FAT_SLICES {
        return Err(FormatError::Limit {
            resource: "Mach-O slices",
            limit: MAX_FAT_SLICES,
        });
    }
    let entry_size = if fat64 { 32 } else { 20 };
    reader.table(8, count, entry_size)?;
    let table_end = 8 + count as u64 * entry_size;
    let mut slices: Vec<FatSlice> = Vec::with_capacity(count);
    let mut fields = vec![HeaderField {
        name: "Fat.SliceCount".into(),
        value: count as u64,
        range: reader.range(4, 4)?,
    }];
    for index in 0..count {
        let at = 8 + index as u64 * entry_size;
        let mut bytes = [0; 32];
        if fat64 {
            bytes = reader.read::<32>(at)?;
        } else {
            bytes[..20].copy_from_slice(&reader.read::<20>(at)?);
        }
        let cpu = u32_at(&bytes, 0, endian);
        let cpu_subtype = u32_at(&bytes, 4, endian);
        let (offset, size, alignment) = if fat64 {
            (
                u64_at(&bytes, 8, endian),
                u64_at(&bytes, 16, endian),
                u32_at(&bytes, 24, endian),
            )
        } else {
            (
                u64::from(u32_at(&bytes, 8, endian)),
                u64::from(u32_at(&bytes, 12, endian)),
                u32_at(&bytes, 16, endian),
            )
        };
        let alignment_bytes = 1_u64
            .checked_shl(alignment)
            .ok_or_else(|| reader.invalid("fat Mach-O alignment exponent is too large", at))?;
        if offset < table_end || offset % alignment_bytes != 0 || size == 0 {
            return Err(reader.invalid(
                "fat Mach-O slice overlaps its table, is empty, or is misaligned",
                at,
            ));
        }
        let file = reader.range(offset, size)?;
        let end = file
            .end()
            .map_err(|_| reader.invalid("fat Mach-O slice range overflow", at))?;
        if slices
            .iter()
            .any(|slice| slice.file.contains(file.offset) || file.contains(slice.file.offset))
        {
            return Err(reader.invalid("fat Mach-O slices overlap", at));
        }
        debug_assert!(end.get() <= reader.document.len());
        slices.push(FatSlice {
            index,
            machine: machine(cpu),
            cpu_type: cpu,
            cpu_subtype,
            file,
        });
        for (label, value, relative, width) in [
            ("CpuType", u64::from(cpu), 0, 4),
            ("CpuSubtype", u64::from(cpu_subtype), 4, 4),
            ("Offset", offset, 8, if fat64 { 8 } else { 4 }),
            (
                "Size",
                size,
                if fat64 { 16 } else { 12 },
                if fat64 { 8 } else { 4 },
            ),
        ] {
            fields.push(HeaderField {
                name: format!("Fat[{index}].{label}"),
                value,
                range: reader.range(at + relative, width)?,
            });
        }
    }
    let index = requested.unwrap_or(0);
    let slice = slices.get(index).ok_or(FormatError::InvalidSlice {
        requested: index,
        count,
    })?;
    reader.slice = slice.file;
    let magic = reader.magic()?;
    if !is_thin(magic) {
        return Err(FormatError::Unsupported(
            "non-Mach-O or nested-fat universal members",
        ));
    }
    let mut result = parse_image(reader, magic, generation)?
        .ok_or(FormatError::Unsupported("unrecognized universal member"))?;
    let member = reader.read::<12>(0)?;
    if u32_at(&member, 4, result.endian) != slice.cpu_type
        || u32_at(&member, 8, result.endian) != slice.cpu_subtype
    {
        return Err(reader.invalid(
            "fat Mach-O CPU metadata differs from the selected member",
            4,
        ));
    }
    result.selected_slice = Some(index);
    result.slices = slices;
    result.fields.extend(fields);
    Ok(result)
}
