use super::{
    BinaryFormat, Endian, Executable, FormatError, Machine, Reader, Region, VirtualAddress, image,
    u16_at, u32_at, u64_at,
};
use crate::ByteSource;

// Image identification and PT_LOAD mappings from the System V gABI. Section
// contents, symbols, relocations, dynamic tags and runtime load bias stay separate.
pub(super) fn parse<S: ByteSource>(
    reader: &mut Reader<'_, S>,
    generation: u64,
) -> Result<Executable, FormatError> {
    let ident = reader.read::<16>(0)?;
    let (format, bitness, header_size, program_size, section_size) = match ident[4] {
        1 => (BinaryFormat::Elf32, 32, 52, 32, 40),
        2 => (BinaryFormat::Elf64, 64, 64, 56, 64),
        _ => return Err(FormatError::Unsupported("this ELF class")),
    };
    let endian = match ident[5] {
        1 => Endian::Little,
        2 => Endian::Big,
        _ => return Err(FormatError::Unsupported("this ELF byte order")),
    };
    if ident[6] != 1 {
        return Err(FormatError::Unsupported("this ELF identification version"));
    }
    let mut header = [0; 64];
    if bitness == 64 {
        header = reader.read::<64>(0)?;
    } else {
        header[..52].copy_from_slice(&reader.read::<52>(0)?);
    }
    if u32_at(&header, 20, endian) != 1 {
        return Err(FormatError::Unsupported("this ELF header version"));
    }
    let raw_machine = u16_at(&header, 18, endian);
    let machine = match raw_machine {
        3 => Machine::X86,
        62 => Machine::X86_64,
        40 => Machine::Arm,
        183 => Machine::Aarch64,
        value => Machine::Other(u32::from(value)),
    };
    let (
        entry,
        phoff,
        shoff,
        ehsize_at,
        phentsize_at,
        phnum_at,
        shentsize_at,
        shnum_at,
        shstrndx_at,
    ) = if bitness == 64 {
        (
            u64_at(&header, 24, endian),
            u64_at(&header, 32, endian),
            u64_at(&header, 40, endian),
            52,
            54,
            56,
            58,
            60,
            62,
        )
    } else {
        (
            u64::from(u32_at(&header, 24, endian)),
            u64::from(u32_at(&header, 28, endian)),
            u64::from(u32_at(&header, 32, endian)),
            40,
            42,
            44,
            46,
            48,
            50,
        )
    };
    let ehsize = u64::from(u16_at(&header, ehsize_at, endian));
    if ehsize < header_size {
        return Err(reader.invalid("ELF declared header is too small", ehsize_at as u64));
    }
    reader.range(0, ehsize)?;
    let phentsize = u64::from(u16_at(&header, phentsize_at, endian));
    let phnum = usize::from(u16_at(&header, phnum_at, endian));
    if phnum != 0 && phentsize < program_size {
        return Err(reader.invalid("ELF program header entry is too small", phentsize_at as u64));
    }
    reader.table(phoff, phnum, phentsize)?;
    let shentsize = u64::from(u16_at(&header, shentsize_at, endian));
    let shnum = usize::from(u16_at(&header, shnum_at, endian));
    let shstrndx = usize::from(u16_at(&header, shstrndx_at, endian));
    if (shnum == 0 && shoff != 0) || shstrndx == 0xffff {
        return Err(FormatError::Unsupported("ELF extended section numbering"));
    }
    if shnum != 0 {
        if shentsize < section_size {
            return Err(
                reader.invalid("ELF section header entry is too small", shentsize_at as u64)
            );
        }
        if shstrndx >= shnum {
            return Err(reader.invalid(
                "ELF section-name table index is outside the section table",
                shstrndx_at as u64,
            ));
        }
        reader.table(shoff, shnum, shentsize)?;
    } else if shstrndx != 0 {
        return Err(reader.invalid(
            "ELF section-name table index has no section table",
            shstrndx_at as u64,
        ));
    }
    let mut result = image(reader, generation, format, machine, endian, bitness);
    result.entry = (entry != 0).then_some(VirtualAddress::new(entry));
    reader.field(&mut result, "ELF.Class", u64::from(ident[4]), 4, 1)?;
    reader.field(&mut result, "ELF.Data", u64::from(ident[5]), 5, 1)?;
    reader.field(
        &mut result,
        "ELF.Type",
        u64::from(u16_at(&header, 16, endian)),
        16,
        2,
    )?;
    reader.field(&mut result, "ELF.Machine", u64::from(raw_machine), 18, 2)?;
    reader.field(&mut result, "ELF.Entry", entry, 24, u64::from(bitness / 8))?;
    reader.field(
        &mut result,
        "ELF.ProgramHeaderOffset",
        phoff,
        if bitness == 64 { 32 } else { 28 },
        u64::from(bitness / 8),
    )?;
    reader.field(
        &mut result,
        "ELF.SectionHeaderOffset",
        shoff,
        if bitness == 64 { 40 } else { 32 },
        u64::from(bitness / 8),
    )?;
    reader.field(
        &mut result,
        "ELF.ProgramHeaderCount",
        phnum as u64,
        phnum_at as u64,
        2,
    )?;
    reader.field(
        &mut result,
        "ELF.SectionHeaderCount",
        shnum as u64,
        shnum_at as u64,
        2,
    )?;
    for index in 0..phnum {
        let at = phoff + index as u64 * phentsize;
        let mut bytes = [0; 56];
        if bitness == 64 {
            bytes = reader.read::<56>(at)?;
        } else {
            bytes[..32].copy_from_slice(&reader.read::<32>(at)?);
        }
        let kind = u32_at(&bytes, 0, endian);
        reader.field(
            &mut result,
            format!("Program[{index}].Type"),
            u64::from(kind),
            at,
            4,
        )?;
        if kind != 1 {
            continue;
        }
        let (offset, address, file_size, memory_size, flags, alignment) = if bitness == 64 {
            (
                u64_at(&bytes, 8, endian),
                u64_at(&bytes, 16, endian),
                u64_at(&bytes, 32, endian),
                u64_at(&bytes, 40, endian),
                u32_at(&bytes, 4, endian),
                u64_at(&bytes, 48, endian),
            )
        } else {
            (
                u64::from(u32_at(&bytes, 4, endian)),
                u64::from(u32_at(&bytes, 8, endian)),
                u64::from(u32_at(&bytes, 16, endian)),
                u64::from(u32_at(&bytes, 20, endian)),
                u32_at(&bytes, 24, endian),
                u64::from(u32_at(&bytes, 28, endian)),
            )
        };
        if file_size > memory_size {
            return Err(reader.invalid("ELF load segment has more file bytes than memory", at));
        }
        if alignment > 1
            && (!alignment.is_power_of_two() || address % alignment != offset % alignment)
        {
            return Err(reader.invalid("ELF load segment has invalid alignment", at));
        }
        if bitness == 32 && memory_size > (1_u64 << 32) - address {
            return Err(reader.invalid("ELF32 virtual range exceeds 32 bits", at));
        }
        reader.region(
            &mut result,
            Region {
                name: format!("LOAD[{index}]"),
                file: reader.range(offset, file_size)?,
                address: VirtualAddress::new(address),
                memory_size,
                readable: flags & 4 != 0,
                writable: flags & 2 != 0,
                executable: flags & 1 != 0,
                header: reader.range(at, program_size)?,
            },
        )?;
        let scalar_size = u64::from(bitness / 8);
        for (label, value, relative) in [
            ("Offset", offset, if bitness == 64 { 8 } else { 4 }),
            (
                "VirtualAddress",
                address,
                if bitness == 64 { 16 } else { 8 },
            ),
            ("FileSize", file_size, if bitness == 64 { 32 } else { 16 }),
            (
                "MemorySize",
                memory_size,
                if bitness == 64 { 40 } else { 20 },
            ),
            ("Alignment", alignment, if bitness == 64 { 48 } else { 28 }),
        ] {
            reader.field(
                &mut result,
                format!("Program[{index}].{label}"),
                value,
                at + relative,
                scalar_size,
            )?;
        }
    }
    Ok(result)
}
