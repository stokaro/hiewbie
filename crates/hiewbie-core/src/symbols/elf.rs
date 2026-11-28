//! Section-backed ELF symbols, decoded from the System V gABI integer fields.
//! Loader relocation, library binding and symbol-version resolution stay separate.

use super::{
    BinaryFormat, Endian, Executable, FormatError, MAX_SYMBOLS, Reader, Symbol, SymbolKind,
    SymbolTable, SymbolTarget, VirtualAddress, u16_at, u32_at, u64_at,
};
use crate::ByteSource;
use crate::formats::{MAX_FORMAT_ENTRIES, Machine};

const SHT_SYMTAB: u32 = 2;
const SHT_STRTAB: u32 = 3;
const SHT_NOBITS: u32 = 8;
const SHT_DYNSYM: u32 = 11;
const SHF_ALLOC: u64 = 2;
const SHF_COMPRESSED: u64 = 0x800;
const SHN_LORESERVE: u16 = 0xff00;
const SHN_ABS: u16 = 0xfff1;
const SHN_COMMON: u16 = 0xfff2;
const SHN_XINDEX: u16 = 0xffff;

#[derive(Clone, Copy)]
struct Section {
    kind: u32,
    flags: u64,
    address: u64,
    offset: u64,
    size: u64,
    link: u32,
    info: u32,
    entry_size: u64,
    header: u64,
}

#[derive(Clone, Copy)]
struct Entry {
    name: u32,
    value: u64,
    size: u64,
    binding: u8,
    kind: u8,
    other: u8,
    section: u16,
}

pub(super) fn parse<S: ByteSource>(
    reader: &mut Reader<'_, S>,
    image: &Executable,
) -> Result<SymbolTable, FormatError> {
    let is_64 = image.format == BinaryFormat::Elf64;
    let endian = image.endian;
    let mut header = [0; 64];
    if is_64 {
        header = reader.read::<64>(0)?;
    } else {
        header[..52].copy_from_slice(&reader.read::<52>(0)?);
    }
    let file_type = u16_at(&header, 16, endian);
    let (section_offset, stride_at, count_at, names_at, section_size) = if is_64 {
        (u64_at(&header, 40, endian), 58, 60, 62, 64)
    } else {
        (u64::from(u32_at(&header, 32, endian)), 46, 48, 50, 40)
    };
    let stride = u64::from(u16_at(&header, stride_at, endian));
    let count = usize::from(u16_at(&header, count_at, endian));
    let names = u16_at(&header, names_at, endian);
    let mut result = SymbolTable::new(image);
    reader.note(
        &mut result,
        "ELF coverage: named SHT_SYMTAB/SHT_DYNSYM entries; loader relocations, library bindings and symbol versions are not resolved.",
    )?;
    if (count == 0 && section_offset != 0) || names == SHN_XINDEX {
        return Err(FormatError::Unsupported("ELF extended section numbering"));
    }
    if count == 0 {
        reader.note(
            &mut result,
            "ELF has no section table; dynamic symbols from PT_DYNAMIC are not decoded. This is not a complete import/export inventory.",
        )?;
        return Ok(result);
    }
    if !matches!(file_type, 1..=3) {
        return Err(FormatError::Unsupported("symbols for this ELF file type"));
    }
    if count > MAX_FORMAT_ENTRIES {
        return Err(FormatError::Limit {
            resource: "ELF symbol sections",
            limit: MAX_FORMAT_ENTRIES,
        });
    }
    if section_offset == 0 || stride < section_size {
        return Err(reader.invalid("invalid ELF section header table", stride_at as u64));
    }
    reader.table(section_offset, count as u64, stride)?;
    let mut sections = Vec::with_capacity(count);
    for index in 0..count {
        // The complete product and range were checked by table().
        let at = section_offset + index as u64 * stride;
        sections.push(read_section(reader, at, is_64, endian)?);
    }
    if sections[0].kind != 0 {
        return Err(reader.invalid("ELF section zero is not SHT_NULL", section_offset));
    }
    if sections
        .iter()
        .any(|section| matches!(section.kind, 0x6fff_fffd..=0x6fff_ffff))
    {
        reader.note(
            &mut result,
            "ELF version sections are present; displayed base names can have different unresolved symbol versions.",
        )?;
    }
    let mut records = 0_u64;
    let mut found_table = false;
    for section in &sections {
        if !matches!(section.kind, SHT_SYMTAB | SHT_DYNSYM) {
            continue;
        }
        found_table = true;
        parse_table(
            reader,
            image,
            &mut result,
            &sections,
            section,
            file_type,
            &mut records,
        )?;
    }
    if !found_table {
        reader.note(
            &mut result,
            "No ELF symbol-table sections were found; sectionless dynamic metadata is not decoded.",
        )?;
    }
    Ok(result)
}

fn read_section<S: ByteSource>(
    reader: &mut Reader<'_, S>,
    at: u64,
    is_64: bool,
    endian: Endian,
) -> Result<Section, FormatError> {
    if is_64 {
        let bytes = reader.read::<64>(at)?;
        Ok(Section {
            kind: u32_at(&bytes, 4, endian),
            flags: u64_at(&bytes, 8, endian),
            address: u64_at(&bytes, 16, endian),
            offset: u64_at(&bytes, 24, endian),
            size: u64_at(&bytes, 32, endian),
            link: u32_at(&bytes, 40, endian),
            info: u32_at(&bytes, 44, endian),
            entry_size: u64_at(&bytes, 56, endian),
            header: at,
        })
    } else {
        let bytes = reader.read::<40>(at)?;
        Ok(Section {
            kind: u32_at(&bytes, 4, endian),
            flags: u64::from(u32_at(&bytes, 8, endian)),
            address: u64::from(u32_at(&bytes, 12, endian)),
            offset: u64::from(u32_at(&bytes, 16, endian)),
            size: u64::from(u32_at(&bytes, 20, endian)),
            link: u32_at(&bytes, 24, endian),
            info: u32_at(&bytes, 28, endian),
            entry_size: u64::from(u32_at(&bytes, 36, endian)),
            header: at,
        })
    }
}

fn parse_table<S: ByteSource>(
    reader: &mut Reader<'_, S>,
    image: &Executable,
    result: &mut SymbolTable,
    sections: &[Section],
    section: &Section,
    file_type: u16,
    records: &mut u64,
) -> Result<(), FormatError> {
    let is_64 = image.format == BinaryFormat::Elf64;
    let entry_size = if is_64 { 24 } else { 16 };
    if section.flags & SHF_COMPRESSED != 0 {
        return Err(FormatError::Unsupported("compressed ELF symbol tables"));
    }
    if section.entry_size < entry_size || !section.size.is_multiple_of(section.entry_size) {
        return Err(reader.invalid("invalid ELF symbol table entry size", section.header));
    }
    if section.entry_size > entry_size {
        reader.note(
            result,
            "ELF symbol entries have an extended stride; record navigation covers the standard fields, and trailing bytes are not interpreted.",
        )?;
    }
    let count = section.size / section.entry_size;
    *records = records
        .checked_add(count)
        .filter(|total| *total <= MAX_SYMBOLS as u64)
        .ok_or(FormatError::Limit {
            resource: "ELF symbol records",
            limit: MAX_SYMBOLS,
        })?;
    reader.table(section.offset, count, section.entry_size)?;
    if u64::from(section.info) > count || (count != 0 && section.info == 0) {
        return Err(reader.invalid(
            "ELF first non-local symbol index is out of range",
            section.header,
        ));
    }
    let strings = sections
        .get(section.link as usize)
        .filter(|strings| strings.kind == SHT_STRTAB)
        .ok_or_else(|| {
            reader.invalid("ELF symbols require a linked string table", section.header)
        })?;
    if strings.flags & SHF_COMPRESSED != 0 {
        return Err(FormatError::Unsupported(
            "compressed ELF symbol string tables",
        ));
    }
    reader.range(strings.offset, strings.size)?;
    if strings.size != 0
        && (reader.read::<1>(strings.offset)?[0] != 0
            || reader.read::<1>(strings.offset + strings.size - 1)?[0] != 0)
    {
        return Err(reader.invalid(
            "ELF string table must start and end with NUL",
            strings.offset,
        ));
    }
    for index in 0..count {
        let at = section.offset + index * section.entry_size;
        let entry = read_entry(reader, at, is_64, image.endian)?;
        if index == 0 {
            if entry.name != 0
                || entry.value != 0
                || entry.size != 0
                || entry.binding != 0
                || entry.kind != 0
                || entry.other != 0
                || entry.section != 0
            {
                return Err(reader.invalid("ELF reserved symbol zero is not empty", at));
            }
            continue;
        }
        if (index < u64::from(section.info)) != (entry.binding == 0) {
            return Err(reader.invalid("ELF local symbols disagree with sh_info", at));
        }
        if entry.section == SHN_XINDEX {
            return Err(FormatError::Unsupported(
                "ELF extended symbol section indices",
            ));
        }
        if entry.section < SHN_LORESERVE && usize::from(entry.section) >= sections.len() {
            return Err(reader.invalid("ELF symbol section index is out of range", at));
        }
        if entry.name == 0 {
            reader.note(
                result,
                "Unnamed ELF entries, including section symbols, are omitted.",
            )?;
            continue;
        }
        if u64::from(entry.name) >= strings.size {
            return Err(reader.invalid("ELF symbol name is outside its string table", at));
        }
        let name = reader.c_string(
            strings.offset + u64::from(entry.name),
            strings.offset + strings.size,
        )?;
        let visible = matches!(entry.other & 3, 0 | 3);
        let external = matches!(entry.binding, 1 | 2);
        let kind = if entry.section == 0 && external {
            SymbolKind::Import
        } else if section.kind == SHT_DYNSYM && external && visible && entry.section != 0 {
            SymbolKind::Export
        } else {
            SymbolKind::Defined
        };
        if entry.binding > 2 || entry.kind > 6 || entry.other & !3 != 0 {
            reader.note(
                result,
                "ELF OS/processor-specific symbol attributes are shown numerically; their binding/type extensions are not resolved.",
            )?;
        }
        let (target, interpretation) = target(reader, image, sections, entry, file_type)?;
        let table_name = if section.kind == SHT_DYNSYM {
            "dynsym"
        } else {
            "symtab"
        };
        let detail = format!(
            "ELF {table_name}; {}; {}; {}; value 0x{:X}; section {}; {interpretation}",
            binding_name(entry.binding),
            type_name(entry.kind),
            visibility_name(entry.other),
            entry.value,
            entry.section,
        );
        reader.push(
            result,
            Symbol {
                name,
                kind,
                library: None,
                ordinal: None,
                target,
                record: reader.range(at, entry_size)?,
                size: Some(entry.size),
                detail,
            },
        )?;
    }
    Ok(())
}

fn read_entry<S: ByteSource>(
    reader: &mut Reader<'_, S>,
    at: u64,
    is_64: bool,
    endian: Endian,
) -> Result<Entry, FormatError> {
    let (name, value, size, info, other, section) = if is_64 {
        let bytes = reader.read::<24>(at)?;
        (
            u32_at(&bytes, 0, endian),
            u64_at(&bytes, 8, endian),
            u64_at(&bytes, 16, endian),
            bytes[4],
            bytes[5],
            u16_at(&bytes, 6, endian),
        )
    } else {
        let bytes = reader.read::<16>(at)?;
        (
            u32_at(&bytes, 0, endian),
            u64::from(u32_at(&bytes, 4, endian)),
            u64::from(u32_at(&bytes, 8, endian)),
            bytes[12],
            bytes[13],
            u16_at(&bytes, 14, endian),
        )
    };
    Ok(Entry {
        name,
        value,
        size,
        binding: info >> 4,
        kind: info & 15,
        other,
        section,
    })
}

fn target<S: ByteSource>(
    reader: &Reader<'_, S>,
    image: &Executable,
    sections: &[Section],
    entry: Entry,
    file_type: u16,
) -> Result<(SymbolTarget, &'static str), FormatError> {
    let unavailable = SymbolTarget::Unavailable;
    match entry.section {
        0 => {
            return Ok((
                unavailable,
                "undefined; no resolved definition or import slot",
            ));
        }
        SHN_ABS => return Ok((unavailable, "absolute value; no section-backed target")),
        SHN_COMMON => return Ok((unavailable, "unallocated common; value is alignment")),
        SHN_LORESERVE.. => {
            return Ok((
                unavailable,
                "reserved section index; target not interpreted",
            ));
        }
        _ => {}
    }
    if entry.kind == 6 {
        return Ok((unavailable, "TLS offset; no process-independent address"));
    }
    if entry.kind == 4 {
        return Ok((unavailable, "source-file symbol; not an address"));
    }
    if entry.kind == 5 && file_type == 1 {
        return Ok((
            unavailable,
            "relocatable common symbol; no allocated target",
        ));
    }
    if entry.kind > 6 {
        return Ok((
            unavailable,
            "OS/processor-specific symbol type; target not interpreted",
        ));
    }
    let section = &sections[usize::from(entry.section)];
    if section.kind == SHT_NOBITS {
        return Ok((unavailable, "zero-fill section; no file-backed target"));
    }
    if section.kind == 0 || section.flags & SHF_COMPRESSED != 0 {
        return Ok((
            unavailable,
            "null or compressed section; no direct file target",
        ));
    }
    reader.range(section.offset, section.size)?;
    let thumb = image.machine == Machine::Arm && entry.kind == 2 && entry.value & 1 != 0;
    let value = if thumb { entry.value & !1 } else { entry.value };
    let relative = if file_type == 1 {
        Some(value)
    } else if section.flags & SHF_ALLOC != 0 {
        value.checked_sub(section.address)
    } else {
        return Ok((
            unavailable,
            "non-allocated section; no mapped virtual target",
        ));
    };
    let Some(relative) = relative
        .filter(|relative| *relative < section.size && entry.size <= section.size - *relative)
    else {
        return Ok((
            unavailable,
            "section boundary or out-of-section value; no backed byte",
        ));
    };
    // The section extent is checked above, and relative is strictly inside it.
    let file = reader.range(section.offset + relative, 1)?.offset;
    if file_type == 1 {
        return Ok((
            SymbolTarget::File(file),
            if thumb {
                "relocatable section offset; Thumb state bit cleared for file target"
            } else {
                "relocatable section offset; no linked virtual address"
            },
        ));
    }
    let address = VirtualAddress::new(value);
    if image.va_to_file(address).ok() != Some(file) {
        return Ok((
            unavailable,
            "section and load mapping do not provide the same unique file byte",
        ));
    }
    Ok((
        if thumb {
            SymbolTarget::Thumb(VirtualAddress::new(entry.value))
        } else if image.machine == Machine::Arm && entry.kind == 2 {
            SymbolTarget::Arm(address)
        } else {
            SymbolTarget::Virtual(address)
        },
        if thumb {
            "Thumb function address"
        } else if image.machine == Machine::Arm && entry.kind == 2 {
            "ARM function address"
        } else {
            "link-time virtual address"
        },
    ))
}

fn binding_name(binding: u8) -> String {
    match binding {
        0 => "local".to_owned(),
        1 => "global".to_owned(),
        2 => "weak".to_owned(),
        value => format!("binding {value}"),
    }
}

fn type_name(kind: u8) -> String {
    match kind {
        0 => "notype".to_owned(),
        1 => "object".to_owned(),
        2 => "function".to_owned(),
        3 => "section".to_owned(),
        4 => "file".to_owned(),
        5 => "common".to_owned(),
        6 => "TLS".to_owned(),
        value => format!("type {value}"),
    }
}

fn visibility_name(other: u8) -> String {
    let name = match other & 3 {
        0 => "default",
        1 => "internal",
        2 => "hidden",
        _ => "protected",
    };
    if other & !3 == 0 {
        name.to_owned()
    } else {
        format!("{name} (other 0x{other:02X})")
    }
}

#[cfg(test)]
#[path = "elf_tests.rs"]
mod tests;
