//! Relocation inspection only: original words, stored values and references.
//! No relocation expression is evaluated or applied to the document.

use super::super::{
    Endian, Executable, FileRange, FormatError, MAX_FORMAT_ENTRIES, Machine, Reader, StructureKind,
    StructureRecord, StructureStatus, u16_at, u32_at, u64_at,
};
use super::commands::{DynamicRelocations, Section, Symtab};
use crate::{ByteSource, FileOffset};

#[derive(Clone, Copy)]
struct Relocation {
    address: u32,
    reference: u32,
    kind: u8,
    length: u8,
    pc_relative: bool,
    external: bool,
    scattered: bool,
}

impl Relocation {
    fn decode(words: [u32; 2], endian: Endian, bitness: u8) -> Self {
        // Scattered records do not exist in x86-64/arm64. In particular, a
        // signed negative 64-bit-architecture r_address is not a scattered tag.
        if bitness == 32 && words[0] & 0x8000_0000 != 0 {
            Self {
                address: words[0] & 0x00ff_ffff,
                reference: words[1],
                kind: ((words[0] >> 24) & 0xf) as u8,
                length: ((words[0] >> 28) & 3) as u8,
                pc_relative: words[0] & 0x4000_0000 != 0,
                external: false,
                scattered: true,
            }
        } else {
            let (reference, pc_relative, length, external, kind) = match endian {
                Endian::Little => (
                    words[1] & 0x00ff_ffff,
                    (words[1] >> 24) & 1,
                    (words[1] >> 25) & 3,
                    (words[1] >> 27) & 1,
                    words[1] >> 28,
                ),
                Endian::Big => (
                    words[1] >> 8,
                    (words[1] >> 7) & 1,
                    (words[1] >> 5) & 3,
                    (words[1] >> 4) & 1,
                    words[1] & 0xf,
                ),
            };
            Self {
                address: words[0],
                reference,
                kind: kind as u8,
                length: length as u8,
                pc_relative: pc_relative != 0,
                external: external != 0,
                scattered: false,
            }
        }
    }

    fn is_pair(self, machine: Machine) -> bool {
        matches!(machine, Machine::X86 | Machine::Arm) && self.kind == 1
    }

    fn is_addend(self, machine: Machine) -> bool {
        machine == Machine::Aarch64 && self.kind == 10
    }

    fn width(self, machine: Machine) -> u64 {
        if machine == Machine::Arm && matches!(self.kind, 5..=9) {
            4
        } else {
            1 << self.length
        }
    }

    fn name(self, machine: Machine) -> Option<&'static str> {
        let names: &[&str] = match machine {
            Machine::X86 => &[
                "GENERIC_RELOC_VANILLA",
                "GENERIC_RELOC_PAIR",
                "GENERIC_RELOC_SECTDIFF",
                "GENERIC_RELOC_PB_LA_PTR",
                "GENERIC_RELOC_LOCAL_SECTDIFF",
                "GENERIC_RELOC_TLV",
            ],
            Machine::X86_64 => &[
                "X86_64_RELOC_UNSIGNED",
                "X86_64_RELOC_SIGNED",
                "X86_64_RELOC_BRANCH",
                "X86_64_RELOC_GOT_LOAD",
                "X86_64_RELOC_GOT",
                "X86_64_RELOC_SUBTRACTOR",
                "X86_64_RELOC_SIGNED_1",
                "X86_64_RELOC_SIGNED_2",
                "X86_64_RELOC_SIGNED_4",
                "X86_64_RELOC_TLV",
            ],
            Machine::Arm => &[
                "ARM_RELOC_VANILLA",
                "ARM_RELOC_PAIR",
                "ARM_RELOC_SECTDIFF",
                "ARM_RELOC_LOCAL_SECTDIFF",
                "ARM_RELOC_PB_LA_PTR",
                "ARM_RELOC_BR24",
                "ARM_THUMB_RELOC_BR22",
                "ARM_THUMB_32BIT_BRANCH",
                "ARM_RELOC_HALF",
                "ARM_RELOC_HALF_SECTDIFF",
            ],
            Machine::Aarch64 => &[
                "ARM64_RELOC_UNSIGNED",
                "ARM64_RELOC_SUBTRACTOR",
                "ARM64_RELOC_BRANCH26",
                "ARM64_RELOC_PAGE21",
                "ARM64_RELOC_PAGEOFF12",
                "ARM64_RELOC_GOT_LOAD_PAGE21",
                "ARM64_RELOC_GOT_LOAD_PAGEOFF12",
                "ARM64_RELOC_POINTER_TO_GOT",
                "ARM64_RELOC_TLVP_LOAD_PAGE21",
                "ARM64_RELOC_TLVP_LOAD_PAGEOFF12",
                "ARM64_RELOC_ADDEND",
            ],
            _ => return None,
        };
        names.get(usize::from(self.kind)).copied()
    }
}

struct Table<'a> {
    label: String,
    offset: u64,
    count: usize,
    section: Option<&'a Section>,
}

pub(super) fn parse<S: ByteSource>(
    reader: &mut Reader<'_, S>,
    image: &mut Executable,
    sections: &[Section],
    symbols: Option<&Symtab>,
    dynamic: &[DynamicRelocations],
) -> Result<(), FormatError> {
    let mut count = 0_usize;
    for (index, section) in sections.iter().enumerate() {
        count = count
            .checked_add(section.relocation_count)
            .filter(|count| *count <= MAX_FORMAT_ENTRIES)
            .ok_or(FormatError::Limit {
                resource: "Mach-O relocations",
                limit: MAX_FORMAT_ENTRIES,
            })?;
        table(
            reader,
            image,
            sections,
            symbols,
            Table {
                label: format!("Section[{}]", index + 1),
                offset: section.relocations,
                count: section.relocation_count,
                section: Some(section),
            },
        )?;
    }
    for dynamic in dynamic {
        count = count
            .checked_add(dynamic.count)
            .filter(|count| *count <= MAX_FORMAT_ENTRIES)
            .ok_or(FormatError::Limit {
                resource: "Mach-O relocations",
                limit: MAX_FORMAT_ENTRIES,
            })?;
        table(
            reader,
            image,
            sections,
            symbols,
            Table {
                label: format!("Dysymtab.{}", dynamic.name),
                offset: dynamic.offset,
                count: dynamic.count,
                section: None,
            },
        )?;
    }
    if count != 0 {
        reader.note(image, "Mach-O relocation values and references are inspected without applying relocations or resolving runtime addresses.")?;
    }
    Ok(())
}

fn table<S: ByteSource>(
    reader: &mut Reader<'_, S>,
    image: &mut Executable,
    sections: &[Section],
    symbols: Option<&Symtab>,
    table: Table<'_>,
) -> Result<(), FormatError> {
    reader.table(table.offset, table.count, 8)?;
    let mut entries = Vec::with_capacity(table.count);
    for index in 0..table.count {
        let at = table.offset + index as u64 * 8;
        let bytes = reader.read::<8>(at)?;
        let words = [
            u32_at(&bytes, 0, image.endian),
            u32_at(&bytes, 4, image.endian),
        ];
        entries.push(Relocation::decode(words, image.endian, image.bitness));
        reader.field(
            image,
            format!("{}.Relocation[{index}].Word0", table.label),
            u64::from(words[0]),
            at,
            4,
        )?;
        reader.field(
            image,
            format!("{}.Relocation[{index}].Word1", table.label),
            u64::from(words[1]),
            at + 4,
            4,
        )?;
    }
    let (partners, problems) = pairs(&entries, image.machine);
    for (index, relocation) in entries.iter().copied().enumerate() {
        let at = table.offset + index as u64 * 8;
        let raw = reader.range(at, 8)?;
        let label = format!("{}.Relocation[{index}]", table.label);
        let type_name = relocation.name(image.machine);
        let mut status = if type_name.is_some() {
            StructureStatus::Interpreted
        } else {
            StructureStatus::Unsupported
        };
        let mut detail = format!(
            "{} type={} {}; address=0x{:X}; pcrel={}; length={}; {}=0x{:X}",
            type_name.unwrap_or("unknown relocation"),
            relocation.kind,
            if relocation.scattered {
                "scattered"
            } else {
                "plain"
            },
            relocation.address,
            relocation.pc_relative,
            relocation.length,
            if relocation.scattered {
                "r_value"
            } else if relocation.external {
                "nlist index"
            } else {
                "section ordinal"
            },
            relocation.reference
        );
        if let Some(partner) = partners[index] {
            detail.push_str(&format!(
                "; paired with [{}] at file 0x{:X}",
                partner,
                reader
                    .range(table.offset + partner as u64 * 8, 8)?
                    .offset
                    .get()
            ));
        }
        if let Some(problem) = problems[index] {
            status = StructureStatus::Unsupported;
            detail.push_str(&format!("; malformed pair: {problem}"));
        }
        let metadata_only =
            relocation.is_pair(image.machine) || relocation.is_addend(image.machine);
        if relocation.is_pair(image.machine) {
            detail.push_str(&format!(
                "; pair payload=0x{:X}, no independent patch address",
                relocation.address
            ));
            if partners[index].is_none() {
                status = StructureStatus::Unsupported;
                detail.push_str("; orphan PAIR");
            }
        } else if relocation.is_addend(image.machine) {
            let addend = ((relocation.reference << 8) as i32) >> 8;
            detail.push_str(&format!(
                "; signed addend={addend}, not a symbol/section index"
            ));
        } else if image.machine == Machine::Arm && matches!(relocation.kind, 8 | 9) {
            detail.push_str(&format!(
                "; {} half of {} instruction; stored width=4",
                if relocation.length & 1 == 0 {
                    "lower"
                } else {
                    "upper"
                },
                if relocation.length & 2 == 0 {
                    "ARM"
                } else {
                    "Thumb"
                }
            ));
        }
        let target = if type_name.is_some() && !metadata_only && problems[index].is_none() {
            match table.section {
                Some(section) => {
                    let target = value_target(reader, section, relocation, image.machine, at)?;
                    if let Some(target) = target {
                        let stored = stored_value(reader, image.endian, target)?;
                        detail.push_str(&format!("; stored=0x{stored:X} ({} bytes)", target.len));
                    } else {
                        detail.push_str("; value has no interpreted file backing (zero-fill or unsupported section type)");
                    }
                    target
                }
                None => {
                    status = StructureStatus::Unsupported;
                    detail.push_str("; dynamic relocation base depends on loader/image variant; patch target is not inferred");
                    None
                }
            }
        } else {
            None
        };
        if type_name.is_none() {
            detail.push_str("; target and reference semantics are unsupported; raw words retained");
        }
        reader.record(
            image,
            StructureRecord {
                name: label.clone(),
                kind: StructureKind::Relocation,
                record: raw,
                target,
                status,
                detail,
            },
        )?;
        if type_name.is_some() && !relocation.is_addend(image.machine) {
            if relocation.scattered {
                scattered_reference(reader, image, sections, relocation.reference, raw, &label)?;
            } else if !relocation.is_pair(image.machine)
                && let Err(error) =
                    reference(reader, image, sections, symbols, relocation, raw, &label)
            {
                match error {
                    FormatError::Invalid { .. } | FormatError::Unsupported(_) => {
                        reader.record(image, StructureRecord {
                                name: format!("{label}.Reference"), kind: StructureKind::Relocation,
                                record: raw, target: None, status: StructureStatus::Unsupported,
                                detail: format!("Reference unavailable: {error}; relocation record and value remain inspectable"),
                            })?;
                    }
                    other => return Err(other),
                }
            }
        }
    }
    Ok(())
}

type Pairing = (Vec<Option<usize>>, Vec<Option<&'static str>>);

fn pairs(entries: &[Relocation], machine: Machine) -> Pairing {
    let mut partners = vec![None; entries.len()];
    let mut problems = vec![None; entries.len()];
    for (index, first) in entries.iter().copied().enumerate() {
        let generic = matches!(
            (machine, first.kind),
            (Machine::X86, 2 | 4) | (Machine::Arm, 2 | 3 | 8 | 9)
        );
        let subtractor = matches!(
            (machine, first.kind),
            (Machine::X86_64, 5) | (Machine::Aarch64, 1)
        );
        let addend = first.is_addend(machine);
        if !generic && !subtractor && !addend {
            continue;
        }
        let Some(next) = entries.get(index + 1).copied() else {
            problems[index] = Some("required following relocation is missing");
            continue;
        };
        let valid = if generic {
            next.kind == 1
        } else if subtractor {
            first.external
                && !first.pc_relative
                && matches!(first.length, 2 | 3)
                && next.kind == 0
                && !next.pc_relative
                && next.length == first.length
                && next.address == first.address
        } else {
            !first.external
                && !first.pc_relative
                && first.length == 2
                && matches!(next.kind, 2..=4)
                && next.address == first.address
                && next.external
                && next.length == 2
                && next.pc_relative == (next.kind != 4)
        };
        if valid {
            partners[index] = Some(index + 1);
            partners[index + 1] = Some(index);
        } else {
            problems[index] =
                Some("following type/address/flags do not satisfy this relocation's pair layout");
            problems[index + 1] =
                Some("preceding relocation does not form a valid pair with this record");
        }
    }
    (partners, problems)
}

fn value_target<S: ByteSource>(
    reader: &Reader<'_, S>,
    section: &Section,
    relocation: Relocation,
    machine: Machine,
    at: u64,
) -> Result<Option<FileRange>, FormatError> {
    if !relocation.scattered && (relocation.address as i32) < 0 {
        return Err(reader.invalid("Mach-O section relocation has a negative offset", at));
    }
    let offset = u64::from(relocation.address);
    let width = relocation.width(machine);
    if offset > section.size || width > section.size - offset {
        return Err(reader.invalid("Mach-O relocation value exceeds its owning section", at));
    }
    Ok(section.file.map(|file| FileRange {
        offset: FileOffset::new(file.offset.get() + offset),
        len: width,
    }))
}

fn stored_value<S: ByteSource>(
    reader: &mut Reader<'_, S>,
    endian: Endian,
    target: FileRange,
) -> Result<u64, FormatError> {
    let at = target.offset.get() - reader.slice.offset.get();
    Ok(match target.len {
        1 => u64::from(reader.read::<1>(at)?[0]),
        2 => u64::from(u16_at(&reader.read::<2>(at)?, 0, endian)),
        4 => u64::from(u32_at(&reader.read::<4>(at)?, 0, endian)),
        8 => u64_at(&reader.read::<8>(at)?, 0, endian),
        _ => unreachable!("relocation widths are 1, 2, 4 or 8"),
    })
}

#[allow(clippy::too_many_arguments)]
fn reference<S: ByteSource>(
    reader: &mut Reader<'_, S>,
    image: &mut Executable,
    sections: &[Section],
    symbols: Option<&Symtab>,
    relocation: Relocation,
    raw: FileRange,
    label: &str,
) -> Result<(), FormatError> {
    let mut status = StructureStatus::Interpreted;
    let (target, detail) = if relocation.external {
        if let Some(symbols) = symbols {
            if let Some(error) = &symbols.unavailable {
                return reader.record(
                    image,
                    StructureRecord {
                        name: format!("{label}.Reference"),
                        kind: StructureKind::Relocation,
                        record: raw,
                        target: None,
                        status: StructureStatus::Unsupported,
                        detail: format!("nlist[{}] unavailable: {error}", relocation.reference),
                    },
                );
            }
            let index = relocation.reference as usize;
            if index >= symbols.count {
                return Err(reader.invalid(
                    "Mach-O relocation nlist index is out of range",
                    raw.offset.get() - reader.slice.offset.get() + 4,
                ));
            }
            let width = if image.bitness == 64 { 16 } else { 12 };
            let at = symbols.offset + index as u64 * width;
            let mut bytes = [0; 16];
            if width == 16 {
                bytes = reader.read::<16>(at)?;
            } else {
                bytes[..12].copy_from_slice(&reader.read::<12>(at)?);
            }
            let string_index = u32_at(&bytes, 0, image.endian);
            let symbol_name = symbol_name(reader, symbols, string_index, at)?;
            let value = if width == 16 {
                u64_at(&bytes, 8, image.endian)
            } else {
                u64::from(u32_at(&bytes, 8, image.endian))
            };
            let section_index = bytes[5];
            let section_name = if section_index == 0 {
                "none"
            } else {
                sections
                    .get(usize::from(section_index - 1))
                    .ok_or_else(|| {
                        reader.invalid(
                            "Mach-O referenced nlist section ordinal is out of range",
                            at + 5,
                        )
                    })?
                    .name
                    .as_str()
            };
            (
                Some(reader.range(at, width)?),
                format!(
                    "nlist[{index}] {symbol_name}; n_type=0x{:02X}; n_sect={section_index} ({section_name}); n_desc=0x{:04X}; n_value=0x{value:X}; target is the raw nlist record, not a resolved runtime address",
                    bytes[4],
                    u16_at(&bytes, 6, image.endian)
                ),
            )
        } else {
            status = StructureStatus::Unsupported;
            (
                None,
                format!(
                    "external nlist[{}] reference cannot be interpreted without LC_SYMTAB",
                    relocation.reference
                ),
            )
        }
    } else if relocation.reference == 0 {
        (
            None,
            "R_ABS: absolute reference, no section or file target".into(),
        )
    } else {
        let section = sections
            .get(relocation.reference as usize - 1)
            .ok_or_else(|| {
                reader.invalid(
                    "Mach-O relocation section ordinal is out of range",
                    raw.offset.get() - reader.slice.offset.get() + 4,
                )
            })?;
        (
            Some(section.header),
            format!(
                "section[{}] {}; VA=0x{:X}; size=0x{:X}; target is its section header",
                relocation.reference, section.name, section.address, section.size
            ),
        )
    };
    reader.record(
        image,
        StructureRecord {
            name: format!("{label}.Reference"),
            kind: StructureKind::Relocation,
            record: raw,
            target,
            status,
            detail,
        },
    )
}

fn scattered_reference<S: ByteSource>(
    reader: &Reader<'_, S>,
    image: &mut Executable,
    sections: &[Section],
    value: u32,
    raw: FileRange,
    label: &str,
) -> Result<(), FormatError> {
    let value = u64::from(value);
    let mut matches = sections.iter().enumerate().filter(|(_, section)| {
        value
            .checked_sub(section.address)
            .is_some_and(|delta| delta < section.size)
    });
    let first = matches.next();
    let (target, detail) = if matches.next().is_some() {
        (
            None,
            format!(
                "scattered r_value=0x{value:X} matches multiple section ranges; reference target is ambiguous"
            ),
        )
    } else if let Some((index, section)) = first {
        (
            Some(section.header),
            format!(
                "scattered r_value=0x{value:X} refers to section[{}] {}+0x{:X}; target is its section header",
                index + 1,
                section.name,
                value - section.address
            ),
        )
    } else {
        (
            None,
            format!("scattered r_value=0x{value:X}; no matching section; no inferred file target"),
        )
    };
    reader.record(
        image,
        StructureRecord {
            name: format!("{label}.Reference"),
            kind: StructureKind::Relocation,
            record: raw,
            target,
            status: StructureStatus::Interpreted,
            detail,
        },
    )
}

fn symbol_name<S: ByteSource>(
    reader: &mut Reader<'_, S>,
    symbols: &Symtab,
    index: u32,
    field: u64,
) -> Result<String, FormatError> {
    if index == 0 {
        return Ok("<unnamed>".into());
    }
    let index = u64::from(index);
    if index >= symbols.strings_size {
        return Err(reader.invalid(
            "Mach-O referenced nlist string index is out of range",
            field,
        ));
    }
    // Names are display metadata. A long valid mangled name is abbreviated,
    // rather than allocating from untrusted input or rejecting the image.
    let available = symbols.strings_size - index;
    let mut bytes = Vec::new();
    for relative in 0..available.min(256) {
        let byte = reader.read::<1>(symbols.strings + index + relative)?[0];
        if byte == 0 {
            return Ok(super::super::name(&bytes));
        }
        bytes.push(byte);
    }
    if available > 256 {
        Ok(format!("{} [name abbreviated]", super::super::name(&bytes)))
    } else {
        Err(reader.invalid(
            "Mach-O referenced nlist name is unterminated",
            symbols.strings + index,
        ))
    }
}
