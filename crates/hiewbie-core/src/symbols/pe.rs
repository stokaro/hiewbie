//! Independently implemented from Microsoft's PE/COFF specification. The ordinary
//! import/export directories describe names and pointer slots, not loaded DLLs.

use super::{
    MAX_SYMBOLS, Reader, Symbol, SymbolKind, SymbolTable, SymbolTarget, u16_at, u32_at, u64_at,
};
use crate::ByteSource;
use crate::formats::{BinaryFormat, Endian, Executable, FormatError, Rva, VirtualAddress};

const LE: Endian = Endian::Little;
const RVA_END: u64 = u32::MAX as u64 + 1;

#[derive(Clone, Copy)]
struct Directory {
    rva: u64,
    size: u64,
}

struct ExportTable<'a> {
    directory: Directory,
    library: Option<String>,
    records: u64,
    addresses: &'a [u8],
    ordinal_base: u32,
}

impl Directory {
    fn present(self) -> bool {
        self.rva != 0 || self.size != 0
    }
}

pub(super) fn parse<S: ByteSource>(
    reader: &mut Reader<'_, S>,
    image: &Executable,
) -> Result<SymbolTable, FormatError> {
    let mut table = SymbolTable::new(image);
    reader.note(
        &mut table,
        "PE: ordinary import and export directories only; debug/PDB symbols are not interpreted.",
    )?;
    let dos = reader.read::<64>(0)?;
    if &dos[..2] != b"MZ" {
        return Err(reader.invalid("invalid PE DOS signature", 0));
    }
    let pe = u64::from(u32_at(&dos, 60, LE));
    let header = reader.read::<24>(pe)?;
    if &header[..4] != b"PE\0\0" {
        return Err(reader.invalid("invalid PE signature", pe));
    }
    if u32_at(&header, 12, LE) != 0 || u32_at(&header, 16, LE) != 0 {
        reader.note(&mut table, "PE COFF symbol tables are not interpreted.")?;
    }
    let optional = pe + 24; // `pe` is a DWORD, so this cannot overflow u64.
    let optional_size = u64::from(u16_at(&header, 20, LE));
    let (minimum, magic, width) = match image.format {
        BinaryFormat::Pe32 => (96, 0x10b, 4),
        BinaryFormat::Pe64 => (112, 0x20b, 8),
        _ => return Err(FormatError::Unsupported("non-PE symbol image")),
    };
    if optional_size < minimum {
        return Err(reader.invalid("truncated PE optional header", optional));
    }
    reader.range(optional, optional_size)?;
    if u16_at(&reader.read::<2>(optional)?, 0, LE) != magic {
        return Err(reader.invalid("PE symbol/image bitness mismatch", optional));
    }
    let directory_count = u64::from(u32_at(&reader.read::<4>(optional + minimum - 4)?, 0, LE));
    if directory_count > (optional_size - minimum) / 8 {
        return Err(reader.invalid("PE directories exceed the optional header", optional));
    }
    let directory_base = optional + minimum;
    let exports = directory(reader, directory_base, directory_count, 0)?;
    let imports = directory(reader, directory_base, directory_count, 1)?;
    let bound = directory(reader, directory_base, directory_count, 11)?;
    let delay = directory(reader, directory_base, directory_count, 13)?;
    if bound.present() {
        reader.note(
            &mut table,
            "PE bound-import directory is not interpreted; bound external addresses are not resolved.",
        )?;
    }
    if delay.present() {
        reader.note(&mut table, "PE delay-load imports are not interpreted.")?;
    }
    if exports.present() {
        parse_exports(reader, image, &mut table, exports)?;
    }
    if imports.present() {
        parse_imports(reader, image, &mut table, imports, width)?;
    }
    Ok(table)
}

fn directory<S: ByteSource>(
    reader: &mut Reader<'_, S>,
    base: u64,
    count: u64,
    index: u64,
) -> Result<Directory, FormatError> {
    if index >= count {
        return Ok(Directory { rva: 0, size: 0 });
    }
    let raw = reader.read::<8>(base + index * 8)?;
    Ok(Directory {
        rva: u64::from(u32_at(&raw, 0, LE)),
        size: u64::from(u32_at(&raw, 4, LE)),
    })
}

fn map<S: ByteSource>(
    reader: &Reader<'_, S>,
    image: &Executable,
    rva: u64,
    length: u64,
) -> Result<u64, FormatError> {
    if rva >= RVA_END || length > RVA_END - rva {
        return Err(reader.invalid("PE symbol RVA span exceeds 32 bits", 0));
    }
    reader.rva(image, rva, length)
}

fn directory_offset<S: ByteSource>(
    reader: &Reader<'_, S>,
    image: &Executable,
    directory: Directory,
    minimum: u64,
) -> Result<u64, FormatError> {
    if directory.rva == 0 || directory.size < minimum {
        return Err(reader.invalid("incomplete PE symbol directory", 0));
    }
    map(reader, image, directory.rva, directory.size)
}

fn address<S: ByteSource>(
    reader: &Reader<'_, S>,
    image: &Executable,
    rva: u64,
) -> Result<VirtualAddress, FormatError> {
    let address = image
        .rva_to_va(Rva::new(rva))
        .map_err(|_| reader.invalid("PE symbol virtual address overflow", 0))?;
    if image.format == BinaryFormat::Pe32 && address.get() > u64::from(u32::MAX) {
        return Err(reader.invalid("PE32 symbol virtual address exceeds 32 bits", 0));
    }
    Ok(address)
}

/// The string stops at the first mapping boundary, overlap, explicit RVA end,
/// or NUL. This prevents a short name from being read into adjacent mappings.
fn name<S: ByteSource>(
    reader: &mut Reader<'_, S>,
    image: &Executable,
    rva: u64,
    limit: u64,
) -> Result<String, FormatError> {
    if rva == 0 || rva >= limit {
        return Err(reader.invalid("invalid PE symbol name RVA", 0));
    }
    let offset = map(reader, image, rva, 1)?;
    let va = address(reader, image, rva)?.get();
    let mut length = limit.min(RVA_END) - rva;
    for region in &image.regions {
        if region.memory_size == 0 {
            continue;
        }
        let start = region.address.get();
        let backed_end = start
            .checked_add(region.memory_size.min(region.file.len))
            .ok_or_else(|| reader.invalid("PE symbol string mapping overflow", offset))?;
        if start <= va && va < backed_end {
            length = length.min(backed_end - va);
        } else if start > va {
            length = length.min(start - va);
        }
    }
    let end = offset
        .checked_add(length)
        .ok_or_else(|| reader.invalid("PE symbol string range overflow", offset))?;
    let text = reader.c_string(offset, end)?;
    if text.is_empty() {
        return Err(reader.invalid("empty PE symbol name", offset));
    }
    Ok(text)
}

fn mapped_table<S: ByteSource>(
    reader: &Reader<'_, S>,
    image: &Executable,
    rva: u64,
    count: u32,
    width: u64,
) -> Result<u64, FormatError> {
    if count as u64 > MAX_SYMBOLS as u64 {
        return Err(FormatError::Limit {
            resource: "symbol table entries",
            limit: MAX_SYMBOLS,
        });
    }
    if count == 0 {
        return Ok(0);
    }
    if rva == 0 {
        return Err(reader.invalid("missing PE symbol table RVA", 0));
    }
    let offset = map(reader, image, rva, u64::from(count) * width)?;
    reader.table(offset, u64::from(count), width)?;
    Ok(offset)
}

fn parse_exports<S: ByteSource>(
    reader: &mut Reader<'_, S>,
    image: &Executable,
    table: &mut SymbolTable,
    directory: Directory,
) -> Result<(), FormatError> {
    let offset = directory_offset(reader, image, directory, 40)?;
    let raw = reader.read::<40>(offset)?;
    let library_rva = u64::from(u32_at(&raw, 12, LE));
    let library = if library_rva == 0 {
        None
    } else {
        Some(name(reader, image, library_rva, RVA_END)?)
    };
    let base = u32_at(&raw, 16, LE);
    let function_count = u32_at(&raw, 20, LE);
    let name_count = u32_at(&raw, 24, LE);
    let functions = mapped_table(
        reader,
        image,
        u64::from(u32_at(&raw, 28, LE)),
        function_count,
        4,
    )?;
    let names = mapped_table(
        reader,
        image,
        u64::from(u32_at(&raw, 32, LE)),
        name_count,
        4,
    )?;
    let ordinals = mapped_table(
        reader,
        image,
        u64::from(u32_at(&raw, 36, LE)),
        name_count,
        2,
    )?;
    if function_count != 0 && base.checked_add(function_count - 1).is_none() {
        return Err(reader.invalid("PE export ordinal overflow", offset + 16));
    }
    let addresses = reader.bytes(functions, function_count as usize * 4)?;
    let name_pointers = reader.bytes(names, name_count as usize * 4)?;
    let ordinal_indexes = reader.bytes(ordinals, name_count as usize * 2)?;
    let exports = ExportTable {
        directory,
        library,
        records: functions,
        addresses: &addresses,
        ordinal_base: base,
    };
    let mut named = vec![false; function_count as usize];
    for index in 0..name_count as usize {
        let ordinal_index = usize::from(u16_at(&ordinal_indexes, index * 2, LE));
        if ordinal_index >= named.len() {
            return Err(reader.invalid(
                "PE export name ordinal is outside the address table",
                ordinals + index as u64 * 2,
            ));
        }
        let export_name = name(
            reader,
            image,
            u64::from(u32_at(&name_pointers, index * 4, LE)),
            RVA_END,
        )?;
        named[ordinal_index] = true;
        push_export(reader, image, table, &exports, ordinal_index, export_name)?;
    }
    for (index, has_name) in named.into_iter().enumerate() {
        if !has_name && u32_at(&addresses, index * 4, LE) != 0 {
            push_export(
                reader,
                image,
                table,
                &exports,
                index,
                format!("#{}", u64::from(base) + index as u64),
            )?;
        }
    }
    Ok(())
}

fn push_export<S: ByteSource>(
    reader: &mut Reader<'_, S>,
    image: &Executable,
    table: &mut SymbolTable,
    exports: &ExportTable<'_>,
    index: usize,
    export_name: String,
) -> Result<(), FormatError> {
    let rva = u64::from(u32_at(exports.addresses, index * 4, LE));
    let directory = exports.directory;
    let directory_end = directory.rva + directory.size; // validated by `map`
    let (target, detail) = if rva == 0 {
        reader.note(
            table,
            "PE named exports with an empty address-table entry have no target.",
        )?;
        (SymbolTarget::Unavailable, "empty export address".to_owned())
    } else if (directory.rva..directory_end).contains(&rva) {
        let forwarder = name(reader, image, rva, directory_end)?;
        (
            SymbolTarget::Unavailable,
            format!("forwarder to {forwarder}; external target unresolved"),
        )
    } else {
        // Virtual targets may legitimately be zero-fill. Navigation separately
        // checks that the address has unambiguous, file-backed bytes.
        (
            SymbolTarget::Virtual(address(reader, image, rva)?),
            "export RVA".to_owned(),
        )
    };
    let record = reader.range(exports.records + index as u64 * 4, 4)?;
    reader.push(
        table,
        Symbol {
            name: export_name,
            kind: SymbolKind::Export,
            library: exports.library.clone(),
            ordinal: Some(u64::from(exports.ordinal_base) + index as u64),
            target,
            record,
            size: None,
            detail,
        },
    )
}

fn parse_imports<S: ByteSource>(
    reader: &mut Reader<'_, S>,
    image: &Executable,
    table: &mut SymbolTable,
    directory: Directory,
    width: u64,
) -> Result<(), FormatError> {
    let offset = directory_offset(reader, image, directory, 20)?;
    for index in 0..=MAX_SYMBOLS {
        let relative = index as u64 * 20;
        if relative > directory.size || directory.size - relative < 20 {
            return Err(reader.invalid("unterminated PE import directory", offset + relative));
        }
        let at = offset + relative;
        let raw = reader.read::<20>(at)?;
        if raw.iter().all(|byte| *byte == 0) {
            return Ok(());
        }
        if index == MAX_SYMBOLS {
            return Err(FormatError::Limit {
                resource: "PE import descriptors",
                limit: MAX_SYMBOLS,
            });
        }
        let lookup = u64::from(u32_at(&raw, 0, LE));
        let bound = u32_at(&raw, 4, LE) != 0;
        let library = name(reader, image, u64::from(u32_at(&raw, 12, LE)), RVA_END)?;
        let iat = u64::from(u32_at(&raw, 16, LE));
        if bound || !matches!(u32_at(&raw, 8, LE), 0 | u32::MAX) {
            reader.note(table, "PE import binding/forwarder-chain metadata is not resolved; targets are IAT pointer slots.")?;
        }
        if lookup == 0 && bound {
            reader.note(table, "PE bound-only imports without a lookup table are omitted: bound IAT values are external addresses, not names or ordinals.")?;
            continue;
        }
        if iat == 0 {
            return Err(reader.invalid("missing PE import address table", at + 16));
        }
        if lookup == 0 {
            reader.note(table, "PE imports without a lookup table use the validated unbound IAT as name/ordinal metadata.")?;
        }
        parse_thunks(
            reader,
            image,
            table,
            if lookup == 0 { iat } else { lookup },
            iat,
            width,
            &library,
        )?;
    }
    Err(FormatError::Limit {
        resource: "PE import descriptors",
        limit: MAX_SYMBOLS,
    })
}

fn parse_thunks<S: ByteSource>(
    reader: &mut Reader<'_, S>,
    image: &Executable,
    table: &mut SymbolTable,
    lookup: u64,
    iat: u64,
    width: u64,
    library: &str,
) -> Result<(), FormatError> {
    let ordinal_flag = if width == 8 { 1u64 << 63 } else { 1u64 << 31 };
    for index in 0..=MAX_SYMBOLS {
        let delta = index as u64 * width;
        let lookup_offset = map(reader, image, lookup + delta, width)?;
        let value = thunk(reader, lookup_offset, width)?;
        let iat_offset = map(reader, image, iat + delta, width)?;
        if value == 0 {
            if lookup != iat && thunk(reader, iat_offset, width)? != 0 {
                return Err(reader.invalid("PE IAT lacks its matching terminator", iat_offset));
            }
            return Ok(());
        }
        if index == MAX_SYMBOLS {
            return Err(FormatError::Limit {
                resource: "PE import thunks",
                limit: MAX_SYMBOLS,
            });
        }
        let (import_name, ordinal, detail) = if value & ordinal_flag != 0 {
            if value & !(ordinal_flag | 0xffff) != 0 {
                return Err(reader.invalid("reserved bits in PE ordinal import", lookup_offset));
            }
            let ordinal = value & 0xffff;
            (
                format!("#{ordinal}"),
                Some(ordinal),
                "ordinal import; IAT pointer slot".to_owned(),
            )
        } else {
            if value > 0x7fff_ffff {
                return Err(reader.invalid(
                    "reserved bits or bound address in PE name import",
                    lookup_offset,
                ));
            }
            let hint_offset = map(reader, image, value, 2)?;
            let hint = u16_at(&reader.read::<2>(hint_offset)?, 0, LE);
            (
                name(reader, image, value + 2, RVA_END)?,
                None,
                format!("hint {hint}; IAT pointer slot"),
            )
        };
        let target = SymbolTarget::ImportSlot(address(reader, image, iat + delta)?);
        let record = reader.range(lookup_offset, width)?;
        reader.push(
            table,
            Symbol {
                name: import_name,
                kind: SymbolKind::Import,
                library: Some(library.to_owned()),
                ordinal,
                target,
                record,
                size: None,
                detail,
            },
        )?;
    }
    Err(FormatError::Limit {
        resource: "PE import thunks",
        limit: MAX_SYMBOLS,
    })
}

fn thunk<S: ByteSource>(
    reader: &mut Reader<'_, S>,
    offset: u64,
    width: u64,
) -> Result<u64, FormatError> {
    if width == 8 {
        Ok(u64_at(&reader.read::<8>(offset)?, 0, LE))
    } else {
        Ok(u64::from(u32_at(&reader.read::<4>(offset)?, 0, LE)))
    }
}

#[cfg(test)]
#[path = "pe_tests.rs"]
mod tests;
