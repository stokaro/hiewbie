use super::{
    BinaryFormat, Endian, Executable, FormatError, Machine, Reader, Region, VirtualAddress, image,
    name, u16_at, u32_at, u64_at,
};
use crate::ByteSource;

// Independently implemented from Microsoft's PE/COFF image layout specification.
// Directories, symbols, relocations, and loader policy are outside this reader.
pub(super) fn parse<S: ByteSource>(
    reader: &mut Reader<'_, S>,
    generation: u64,
) -> Result<Executable, FormatError> {
    let dos = reader.read::<64>(0)?;
    let pe = u64::from(u32_at(&dos, 60, Endian::Little));
    if pe < 64 {
        return Err(reader.invalid("PE header overlaps the DOS header", pe));
    }
    let header = reader.read::<24>(pe)?;
    if &header[..4] != b"PE\0\0" {
        return Err(FormatError::Unsupported(
            "DOS MZ/NE/LE/LX images without a PE signature",
        ));
    }
    let raw_machine = u16_at(&header, 4, Endian::Little);
    let machine = match raw_machine {
        0x14c => Machine::X86,
        0x8664 => Machine::X86_64,
        0x1c0 => Machine::Arm,
        0x1c2 | 0x1c4 => Machine::Thumb,
        0xaa64 => Machine::Aarch64,
        value => Machine::Other(u32::from(value)),
    };
    let count = usize::from(u16_at(&header, 6, Endian::Little));
    let optional_size = u64::from(u16_at(&header, 20, Endian::Little));
    let optional = pe + 24;
    reader.range(optional, optional_size)?;
    if optional_size < 64 {
        return Err(reader.invalid("PE optional header is too short", optional));
    }
    let bytes = reader.read::<64>(optional)?;
    let magic = u16_at(&bytes, 0, Endian::Little);
    let (format, bitness, minimum, base_offset, base_size, base) = match magic {
        0x10b => (
            BinaryFormat::Pe32,
            32,
            96,
            28,
            4,
            u64::from(u32_at(&bytes, 28, Endian::Little)),
        ),
        0x20b => (
            BinaryFormat::Pe64,
            64,
            112,
            24,
            8,
            u64_at(&bytes, 24, Endian::Little),
        ),
        _ => return Err(FormatError::Unsupported("this PE optional-header magic")),
    };
    if optional_size < minimum {
        return Err(reader.invalid("PE optional header is too short", optional));
    }
    if matches!(
        (machine, bitness),
        (Machine::X86_64 | Machine::Aarch64, 32)
            | (Machine::X86 | Machine::Arm | Machine::Thumb, 64)
    ) {
        return Err(reader.invalid("PE machine and optional-header width disagree", pe + 4));
    }
    let directory_bytes = reader.read::<4>(optional + minimum - 4)?;
    let directories = u64::from(u32_at(&directory_bytes, 0, Endian::Little));
    if directories > (optional_size - minimum) / 8 {
        return Err(reader.invalid(
            "PE directory count exceeds the optional header",
            optional + minimum - 4,
        ));
    }
    let table = optional + optional_size;
    reader.table(table, count, 40)?;
    let headers_size = u64::from(u32_at(&bytes, 60, Endian::Little));
    let memory_size = u64::from(u32_at(&bytes, 56, Endian::Little));
    if headers_size < table + count as u64 * 40 || headers_size > memory_size {
        return Err(reader.invalid(
            "PE header size does not contain the headers or exceeds image size",
            optional + 60,
        ));
    }
    base.checked_add(memory_size)
        .ok_or_else(|| reader.invalid("PE image address overflow", optional + base_offset))?;
    if bitness == 32 && memory_size > (1_u64 << 32) - base {
        return Err(reader.invalid(
            "PE32 virtual image range exceeds 32 bits",
            optional + base_offset,
        ));
    }
    let mut result = image(reader, generation, format, machine, Endian::Little, bitness);
    result.image_base = Some(VirtualAddress::new(base));
    let entry = u64::from(u32_at(&bytes, 16, Endian::Little));
    if entry != 0 {
        if entry >= memory_size {
            return Err(reader.invalid("PE entry RVA exceeds the image", optional + 16));
        }
        result.entry = Some(VirtualAddress::new(base + entry));
    }
    reader.field(&mut result, "DOS.e_lfanew", pe, 60, 4)?;
    reader.field(
        &mut result,
        "COFF.Machine",
        u64::from(raw_machine),
        pe + 4,
        2,
    )?;
    reader.field(
        &mut result,
        "COFF.NumberOfSections",
        count as u64,
        pe + 6,
        2,
    )?;
    reader.field(
        &mut result,
        "COFF.SizeOfOptionalHeader",
        optional_size,
        pe + 20,
        2,
    )?;
    reader.field(&mut result, "Optional.Magic", u64::from(magic), optional, 2)?;
    reader.field(
        &mut result,
        "Optional.ImageBase",
        base,
        optional + base_offset,
        base_size,
    )?;
    reader.field(
        &mut result,
        "Optional.AddressOfEntryPoint",
        entry,
        optional + 16,
        4,
    )?;
    reader.field(
        &mut result,
        "Optional.SizeOfImage",
        memory_size,
        optional + 56,
        4,
    )?;
    reader.field(
        &mut result,
        "Optional.SizeOfHeaders",
        headers_size,
        optional + 60,
        4,
    )?;
    reader.field(
        &mut result,
        "Optional.NumberOfRvaAndSizes",
        directories,
        optional + minimum - 4,
        4,
    )?;
    reader.region(
        &mut result,
        Region {
            name: "headers".into(),
            file: reader.range(0, headers_size)?,
            address: VirtualAddress::new(base),
            memory_size: headers_size,
            readable: true,
            writable: false,
            executable: false,
            header: reader.range(optional, optional_size)?,
        },
    )?;
    for index in 0..count {
        let at = table + index as u64 * 40;
        let section = reader.read::<40>(at)?;
        let virtual_size = u64::from(u32_at(&section, 8, Endian::Little));
        let rva = u64::from(u32_at(&section, 12, Endian::Little));
        let raw_size = u64::from(u32_at(&section, 16, Endian::Little));
        let raw_offset = u64::from(u32_at(&section, 20, Endian::Little));
        let flags = u32_at(&section, 36, Endian::Little);
        if rva > memory_size || virtual_size > memory_size - rva {
            return Err(reader.invalid("PE section exceeds the declared image size", at + 8));
        }
        reader.region(
            &mut result,
            Region {
                name: name(&section[..8]),
                file: reader.range(raw_offset, raw_size)?,
                address: VirtualAddress::new(base + rva),
                memory_size: virtual_size,
                readable: flags & 0x4000_0000 != 0,
                writable: flags & 0x8000_0000 != 0,
                executable: flags & 0x2000_0000 != 0,
                header: reader.range(at, 40)?,
            },
        )?;
        for (label, value, offset) in [
            ("VirtualSize", virtual_size, 8),
            ("VirtualAddress", rva, 12),
            ("SizeOfRawData", raw_size, 16),
            ("PointerToRawData", raw_offset, 20),
            ("Characteristics", u64::from(flags), 36),
        ] {
            reader.field(
                &mut result,
                format!("Section[{index}].{label}"),
                value,
                at + offset,
                4,
            )?;
        }
    }
    Ok(result)
}
