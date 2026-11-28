//! Independently implemented from Apple's published Mach-O layout definitions.
//! Nlist definitions are not a substitute for dyld's export and binding metadata.

use super::{Reader, Symbol, SymbolKind, SymbolTable, SymbolTarget, u16_at, u32_at, u64_at};
use crate::ByteSource;
use crate::formats::{Executable, FormatError, MAX_FORMAT_ENTRIES, Machine, VirtualAddress};

const LC_SEGMENT: u32 = 0x1;
const LC_SYMTAB: u32 = 0x2;
const LC_DYSYMTAB: u32 = 0xb;
const LC_LOAD_DYLIB: u32 = 0xc;
const LC_LOAD_WEAK_DYLIB: u32 = 0x8000_0018;
const LC_SEGMENT_64: u32 = 0x19;
const LC_REEXPORT_DYLIB: u32 = 0x8000_001f;
const LC_LAZY_LOAD_DYLIB: u32 = 0x20;
const LC_DYLD_INFO: u32 = 0x22;
const LC_DYLD_INFO_ONLY: u32 = 0x8000_0022;
const LC_LOAD_UPWARD_DYLIB: u32 = 0x8000_0023;
const LC_DYLD_EXPORTS_TRIE: u32 = 0x8000_0033;
const LC_DYLD_CHAINED_FIXUPS: u32 = 0x8000_0034;

const N_STAB: u8 = 0xe0;
const N_PEXT: u8 = 0x10;
const N_TYPE: u8 = 0x0e;
const N_EXT: u8 = 0x01;
const N_UNDF: u8 = 0x0;
const N_ABS: u8 = 0x2;
const N_INDR: u8 = 0xa;
const N_PBUD: u8 = 0xc;
const N_SECT: u8 = 0xe;

const EXPORT_TRIE_NOTE: &str = "Mach-O export tries are not interpreted; nlist external definitions are not a complete runtime export list.";

struct Symtab {
    offset: u64,
    count: u64,
    strings: u64,
    strings_size: u64,
}

struct Section {
    address: u64,
    size: u64,
    file: u64,
    kind: u8,
}

impl Section {
    fn zero_fill(&self) -> bool {
        matches!(self.kind, 0x1 | 0xc | 0x12)
    }

    fn thread_local(&self) -> bool {
        matches!(self.kind, 0x11..=0x15)
    }
}

#[derive(Default)]
struct Metadata {
    symtab: Option<Symtab>,
    sections: Vec<Section>,
    libraries: Vec<String>,
    two_level: bool,
    relocatable: bool,
}

pub(super) fn parse<S: ByteSource>(
    reader: &mut Reader<'_, S>,
    image: &Executable,
) -> Result<SymbolTable, FormatError> {
    let mut result = SymbolTable::new(image);
    let metadata = load_commands(reader, image, &mut result)?;
    let Some(symtab) = &metadata.symtab else {
        reader.note(
            &mut result,
            "No Mach-O LC_SYMTAB is present; an empty nlist view does not establish that the image has no imports or exports.",
        )?;
        return Ok(result);
    };
    let width = if image.bitness == 64 { 16 } else { 12 };
    reader.table(symtab.offset, symtab.count, width)?;
    reader.range(symtab.strings, symtab.strings_size)?;
    let strings_end = symtab.strings + symtab.strings_size;
    reader.note(
        &mut result,
        "Mach-O exports here are public external nlist definitions; imports are unresolved nlist names, not resolved addresses or pointer slots.",
    )?;

    for index in 0..symtab.count {
        let at = symtab.offset + index * width;
        let mut bytes = [0; 16];
        if image.bitness == 64 {
            bytes = reader.read::<16>(at)?;
        } else {
            bytes[..12].copy_from_slice(&reader.read::<12>(at)?);
        }
        let name_index = u64::from(u32_at(&bytes, 0, image.endian));
        let flags = bytes[4];
        if flags & N_STAB != 0 {
            reader.note(
                &mut result,
                "Mach-O STABS debugging records are omitted from the symbol view.",
            )?;
            continue;
        }
        if name_index == 0 {
            reader.note(
                &mut result,
                "Unnamed Mach-O nlist entries are omitted from the symbol view.",
            )?;
            continue;
        }
        let name = string(reader, symtab, name_index, strings_end, at)?;
        if name.is_empty() {
            reader.note(
                &mut result,
                "Unnamed Mach-O nlist entries are omitted from the symbol view.",
            )?;
            continue;
        }
        let section = bytes[5];
        let description = u16_at(&bytes, 6, image.endian);
        let value = if image.bitness == 64 {
            u64_at(&bytes, 8, image.endian)
        } else {
            u64::from(u32_at(&bytes, 8, image.endian))
        };
        let symbol_type = flags & N_TYPE;
        if matches!(symbol_type, N_UNDF | N_ABS | N_INDR | N_PBUD) && section != 0 {
            return Err(reader.invalid("Mach-O non-section symbol has a section ordinal", at + 5));
        }
        let external = flags & N_EXT != 0;
        let private = flags & N_PEXT != 0;
        let definition = if external && !private {
            SymbolKind::Export
        } else {
            SymbolKind::Defined
        };
        let mut symbol = Symbol {
            name,
            kind: SymbolKind::Defined,
            library: None,
            ordinal: None,
            target: SymbolTarget::Unavailable,
            record: reader.range(at, width)?,
            size: None,
            detail: String::new(),
        };

        match symbol_type {
            N_UNDF if external && value != 0 => {
                // For common symbols the descriptor's high byte is alignment,
                // not a dependency ordinal, and n_value is a size, not an address.
                symbol.size = Some(value);
                let alignment = (description >> 8) & 0xf;
                symbol.detail = format!("common allocation, {value} bytes");
                if alignment != 0 {
                    symbol
                        .detail
                        .push_str(&format!(", alignment 2^{alignment}"));
                }
            }
            N_UNDF | N_PBUD => {
                symbol.detail = if symbol_type == N_PBUD {
                    "prebound undefined name; runtime target unavailable".into()
                } else {
                    "undefined name; runtime target unavailable".into()
                };
                if external {
                    symbol.kind = SymbolKind::Import;
                    import_library(reader, &metadata, description, at, &mut symbol)?;
                }
            }
            N_SECT => {
                symbol.kind = definition;
                let section_info = section
                    .checked_sub(1)
                    .and_then(|ordinal| metadata.sections.get(usize::from(ordinal)))
                    .ok_or_else(|| {
                        reader.invalid("Mach-O symbol section ordinal is invalid", at + 5)
                    })?;
                section_target(
                    reader,
                    image,
                    &metadata,
                    section_info,
                    section,
                    value,
                    description,
                    at,
                    &mut symbol,
                )?;
            }
            N_ABS => {
                symbol.kind = definition;
                symbol.detail = format!("absolute value 0x{value:X}; no file address");
            }
            N_INDR => {
                symbol.kind = definition;
                let alias = string(reader, symtab, value, strings_end, at + 8)?;
                symbol.detail = format!("indirect alias to {alias}; target resolution unavailable");
            }
            _ => {
                symbol.detail =
                    format!("unsupported nlist type 0x{symbol_type:X}; no interpreted target");
                reader.note(
                    &mut result,
                    "Some Mach-O nlist symbol types are not interpreted.",
                )?;
            }
        }
        if private {
            symbol.detail.push_str("; private external");
        }
        if symbol.kind == SymbolKind::Import {
            if description & 0x40 != 0 {
                symbol.detail.push_str("; weak reference");
            }
            if description & 0x80 != 0 {
                symbol.detail.push_str("; reference to weak definition");
            }
        } else if matches!(symbol_type, N_SECT | N_ABS | N_INDR) && description & 0x80 != 0 {
            symbol.detail.push_str("; weak definition");
        }
        reader.push(&mut result, symbol)?;
    }
    Ok(result)
}

fn string<S: ByteSource>(
    reader: &mut Reader<'_, S>,
    symtab: &Symtab,
    index: u64,
    end: u64,
    field: u64,
) -> Result<String, FormatError> {
    if index >= symtab.strings_size {
        return Err(reader.invalid(
            "Mach-O symbol string index is outside its string table",
            field,
        ));
    }
    reader.c_string(symtab.strings + index, end)
}

fn load_commands<S: ByteSource>(
    reader: &mut Reader<'_, S>,
    image: &Executable,
    result: &mut SymbolTable,
) -> Result<Metadata, FormatError> {
    let header = reader.read::<28>(0)?;
    let endian = image.endian;
    let header_size = if image.bitness == 64 { 32 } else { 28 };
    let magic = if image.bitness == 64 {
        0xfeed_facf
    } else {
        0xfeed_face
    };
    if u32_at(&header, 0, endian) != magic {
        return Err(reader.invalid("Mach-O symbol metadata does not match the image header", 0));
    }
    let count = u32_at(&header, 16, endian);
    if u64::from(count) > MAX_FORMAT_ENTRIES as u64 {
        return Err(FormatError::Limit {
            resource: "Mach-O symbol load commands",
            limit: MAX_FORMAT_ENTRIES,
        });
    }
    let commands_size = u64::from(u32_at(&header, 20, endian));
    reader.range(header_size, commands_size)?;
    let end = header_size + commands_size;
    let mut at = header_size;
    let mut metadata = Metadata {
        two_level: u32_at(&header, 24, endian) & 0x80 != 0,
        relocatable: u32_at(&header, 12, endian) == 1,
        ..Metadata::default()
    };
    for _ in 0..count {
        if end - at < 8 {
            return Err(reader.invalid("Mach-O symbol load-command header is truncated", at));
        }
        let command = reader.read::<8>(at)?;
        let kind = u32_at(&command, 0, endian);
        let size = u64::from(u32_at(&command, 4, endian));
        let alignment = if image.bitness == 64 { 8 } else { 4 };
        if size < 8 || size % alignment != 0 || size > end - at {
            return Err(reader.invalid(
                "Mach-O symbol load command has invalid size or alignment",
                at,
            ));
        }
        match kind {
            LC_SYMTAB => {
                if size != 24 || metadata.symtab.is_some() {
                    return Err(
                        reader.invalid("Mach-O LC_SYMTAB is duplicated or has the wrong size", at)
                    );
                }
                let fields = reader.read::<24>(at)?;
                metadata.symtab = Some(Symtab {
                    offset: u64::from(u32_at(&fields, 8, endian)),
                    count: u64::from(u32_at(&fields, 12, endian)),
                    strings: u64::from(u32_at(&fields, 16, endian)),
                    strings_size: u64::from(u32_at(&fields, 20, endian)),
                });
            }
            LC_SEGMENT | LC_SEGMENT_64 => {
                if (kind == LC_SEGMENT_64) != (image.bitness == 64) {
                    return Err(reader.invalid("Mach-O segment and symbol widths disagree", at));
                }
                sections(reader, image, at, size, &mut metadata.sections)?;
            }
            LC_LOAD_DYLIB | LC_LOAD_WEAK_DYLIB | LC_REEXPORT_DYLIB | LC_LAZY_LOAD_DYLIB
            | LC_LOAD_UPWARD_DYLIB => {
                if size < 24 {
                    return Err(reader.invalid("Mach-O dylib command is too short", at));
                }
                let fields = reader.read::<24>(at)?;
                let name_offset = u64::from(u32_at(&fields, 8, endian));
                if name_offset < 24 || name_offset >= size {
                    return Err(
                        reader.invalid("Mach-O dylib name is outside its load command", at + 8)
                    );
                }
                let library = reader.c_string(at + name_offset, at + size)?;
                if library.is_empty() {
                    return Err(reader.invalid("Mach-O dylib name is empty", at + name_offset));
                }
                // These names remain live while nlist entries clone their owners.
                reader.retain_text(library.len())?;
                metadata.libraries.push(library);
            }
            LC_DYSYMTAB => {
                if size != 80 {
                    return Err(reader.invalid("Mach-O LC_DYSYMTAB has the wrong size", at));
                }
                reader.note(result, "Mach-O LC_DYSYMTAB indirect symbol/stub bindings are not interpreted; nlist imports have no binding-slot targets.")?;
            }
            LC_DYLD_INFO | LC_DYLD_INFO_ONLY => {
                if size != 48 {
                    return Err(reader.invalid("Mach-O dyld info command has the wrong size", at));
                }
                let fields = reader.read::<48>(at)?;
                for field in (8..48).step_by(8) {
                    let offset = u64::from(u32_at(&fields, field, endian));
                    let length = u64::from(u32_at(&fields, field + 4, endian));
                    if length != 0 {
                        reader.range(offset, length)?;
                    }
                }
                reader.note(result, "Mach-O dyld bind/rebase streams are not interpreted; runtime imports and fixups may be absent from the nlist view.")?;
                reader.note(result, EXPORT_TRIE_NOTE)?;
            }
            LC_DYLD_EXPORTS_TRIE | LC_DYLD_CHAINED_FIXUPS => {
                if size != 16 {
                    return Err(
                        reader.invalid("Mach-O dyld link-edit command has the wrong size", at)
                    );
                }
                let fields = reader.read::<16>(at)?;
                let offset = u64::from(u32_at(&fields, 8, endian));
                let length = u64::from(u32_at(&fields, 12, endian));
                if length != 0 {
                    reader.range(offset, length)?;
                }
                reader.note(result, if kind == LC_DYLD_EXPORTS_TRIE {
                    EXPORT_TRIE_NOTE
                } else {
                    "Mach-O chained fixups are not interpreted; their import names and binding slots may be absent from the nlist view."
                })?;
            }
            _ => {}
        }
        at += size;
    }
    if at != end {
        return Err(reader.invalid(
            "Mach-O command count does not consume the symbol load-command table",
            at,
        ));
    }
    Ok(metadata)
}

fn sections<S: ByteSource>(
    reader: &mut Reader<'_, S>,
    image: &Executable,
    at: u64,
    size: u64,
    result: &mut Vec<Section>,
) -> Result<(), FormatError> {
    let endian = image.endian;
    let wide = image.bitness == 64;
    let (segment_size, section_size) = if wide { (72, 80) } else { (56, 68) };
    if size < segment_size {
        return Err(reader.invalid(
            "Mach-O segment command is too short for symbol sections",
            at,
        ));
    }
    let mut fields = [0; 72];
    if wide {
        fields = reader.read::<72>(at)?;
    } else {
        fields[..56].copy_from_slice(&reader.read::<56>(at)?);
    }
    let (address, memory_size, file, file_size, count) = if wide {
        (
            u64_at(&fields, 24, endian),
            u64_at(&fields, 32, endian),
            u64_at(&fields, 40, endian),
            u64_at(&fields, 48, endian),
            u32_at(&fields, 64, endian),
        )
    } else {
        (
            u64::from(u32_at(&fields, 24, endian)),
            u64::from(u32_at(&fields, 28, endian)),
            u64::from(u32_at(&fields, 32, endian)),
            u64::from(u32_at(&fields, 36, endian)),
            u32_at(&fields, 48, endian),
        )
    };
    if u64::from(count) > MAX_FORMAT_ENTRIES.saturating_sub(result.len()) as u64 {
        return Err(FormatError::Limit {
            resource: "Mach-O symbol sections",
            limit: MAX_FORMAT_ENTRIES,
        });
    }
    if size != segment_size + u64::from(count) * section_size {
        return Err(reader.invalid(
            "Mach-O symbol section count does not match the segment command",
            at,
        ));
    }
    let memory_end = address
        .checked_add(memory_size)
        .ok_or_else(|| reader.invalid("Mach-O segment address overflows", at))?;
    reader.range(file, file_size)?;
    let file_end = file + file_size;
    for index in 0..u64::from(count) {
        let section_at = at + segment_size + index * section_size;
        let mut fields = [0; 80];
        if wide {
            fields = reader.read::<80>(section_at)?;
        } else {
            fields[..68].copy_from_slice(&reader.read::<68>(section_at)?);
        }
        let section = if wide {
            Section {
                address: u64_at(&fields, 32, endian),
                size: u64_at(&fields, 40, endian),
                file: u64::from(u32_at(&fields, 48, endian)),
                kind: u32_at(&fields, 64, endian) as u8,
            }
        } else {
            Section {
                address: u64::from(u32_at(&fields, 32, endian)),
                size: u64::from(u32_at(&fields, 36, endian)),
                file: u64::from(u32_at(&fields, 40, endian)),
                kind: u32_at(&fields, 56, endian) as u8,
            }
        };
        let section_end = section
            .address
            .checked_add(section.size)
            .ok_or_else(|| reader.invalid("Mach-O section address overflows", section_at))?;
        if section.address < address || section_end > memory_end {
            return Err(reader.invalid(
                "Mach-O symbol section is outside its segment memory",
                section_at,
            ));
        }
        if !section.zero_fill() && section.size != 0 {
            if section.file < file
                || section.file > file_end
                || section.size > file_end - section.file
            {
                return Err(reader.invalid(
                    "Mach-O symbol section is outside its segment file bytes",
                    section_at,
                ));
            }
            reader.range(section.file, section.size)?;
        }
        result.push(section);
    }
    Ok(())
}

fn import_library<S: ByteSource>(
    reader: &Reader<'_, S>,
    metadata: &Metadata,
    description: u16,
    at: u64,
    symbol: &mut Symbol,
) -> Result<(), FormatError> {
    if !metadata.two_level {
        symbol.detail.push_str("; flat namespace");
        return Ok(());
    }
    let ordinal = u64::from(description >> 8);
    symbol.ordinal = Some(ordinal);
    symbol.library = Some(match ordinal {
        0 => "<self>".to_owned(),
        0xff => "<main executable>".to_owned(),
        // 0xfe was a real library ordinal before dynamic lookup was added.
        0xfe if metadata.libraries.len() < 0xfe => "<dynamic lookup>".to_owned(),
        _ => metadata
            .libraries
            .get((ordinal - 1) as usize)
            .ok_or_else(|| {
                reader.invalid(
                    "Mach-O symbol library ordinal is outside the dylib list",
                    at + 6,
                )
            })?
            .clone(),
    });
    symbol.detail.push_str("; two-level library ordinal");
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn section_target<S: ByteSource>(
    reader: &Reader<'_, S>,
    image: &Executable,
    metadata: &Metadata,
    section: &Section,
    ordinal: u8,
    value: u64,
    description: u16,
    at: u64,
    symbol: &mut Symbol,
) -> Result<(), FormatError> {
    let delta = value
        .checked_sub(section.address)
        .filter(|delta| *delta <= section.size)
        .ok_or_else(|| {
            reader.invalid(
                "Mach-O symbol value is outside its declared section",
                at + 8,
            )
        })?;
    symbol.detail = format!("section {ordinal}");
    if section.thread_local() {
        symbol
            .detail
            .push_str("; thread-local storage, no process address");
    } else if section.zero_fill() {
        symbol.detail.push_str("; zero-fill storage, no file bytes");
    } else if delta == section.size {
        symbol
            .detail
            .push_str("; end-of-section label, no target byte");
    } else if section.kind > 0x16 {
        symbol
            .detail
            .push_str("; unsupported section type, no interpreted target");
    } else if metadata.relocatable && description & 0x100 != 0 {
        symbol
            .detail
            .push_str("; symbol resolver, runtime result unavailable");
    } else {
        // Section validation already proves this addition remains in its segment.
        let file = section.file + delta;
        let thumb =
            matches!(image.machine, Machine::Arm | Machine::Thumb) && description & 0x8 != 0;
        if thumb && value & 1 != 0 {
            symbol
                .detail
                .push_str("; unaligned Thumb definition, no interpreted target");
        } else if metadata.relocatable && !thumb {
            // MH_OBJECT addresses are link-time section coordinates, even when
            // the current section mappings happen to be unambiguous.
            symbol.target = SymbolTarget::File(reader.range(file, 1)?.offset);
            symbol.detail.push_str("; relocatable file bytes");
        } else if reader.va(image, VirtualAddress::new(value), 1).ok() == Some(file) {
            symbol.target = if thumb {
                symbol.detail.push_str("; Thumb definition");
                SymbolTarget::Thumb(VirtualAddress::new(value))
            } else {
                SymbolTarget::Virtual(VirtualAddress::new(value))
            };
        } else {
            symbol
                .detail
                .push_str("; no unambiguous file-backed image address");
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "macho_tests.rs"]
mod tests;
