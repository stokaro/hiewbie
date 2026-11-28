use super::{
    Endian, Executable, FatSlice, FormatError, HeaderField, MAX_FAT_SLICES, MAX_FORMAT_FIELDS,
    Machine, Reader, parse_image, u32_at, u64_at,
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

mod commands;
mod relocations;
mod threads;

pub(super) use commands::parse;

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
    if result
        .fields
        .len()
        .checked_add(fields.len())
        .is_none_or(|count| count > MAX_FORMAT_FIELDS)
    {
        return Err(FormatError::Limit {
            resource: "header fields",
            limit: MAX_FORMAT_FIELDS,
        });
    }
    result.fields.extend(fields);
    Ok(result)
}
