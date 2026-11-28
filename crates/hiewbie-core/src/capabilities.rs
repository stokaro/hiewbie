//! Versioned descriptions of compiled capabilities, independent of host CPU.
//!
//! Availability describes implementation scope, not successful execution on a
//! particular release target. Research probes are never production backends.

use std::fmt;

use serde::Serialize;

pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_CAPABILITIES: usize = 128;
const MAX_ID_BYTES: usize = 96;
const MAX_TEXT_BYTES: usize = 512;
const MAX_DETAIL_ITEMS: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Category {
    Raw,
    Decode,
    Assembly,
    References,
    Symbols,
    Format,
    Extensions,
    Devices,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    /// The named operation is implemented within its described scope.
    Available,
    /// A usable implementation has the listed incomplete coverage.
    Limited,
    /// No production implementation is loaded for this operation.
    Unavailable,
}

impl Status {
    #[must_use]
    pub const fn is_usable(self) -> bool {
        !matches!(self, Self::Unavailable)
    }
}

impl fmt::Display for Status {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        output.write_str(match self {
            Self::Available => "available",
            Self::Limited => "limited",
            Self::Unavailable => "unavailable",
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct Backend<'a> {
    pub name: &'a str,
    pub version: &'a str,
}

/// A host-supplied description for one operation, not permission to execute it.
///
/// Slices and strings are borrowed from backend-owned metadata. This descriptor
/// is deliberately not deserializable: an untrusted manifest cannot register a
/// backend, grant a device capability, or provide an executable implementation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct Capability<'a> {
    pub id: &'a str,
    pub category: Category,
    pub status: Status,
    pub backend: Option<Backend<'a>>,
    pub modes: &'a [&'a str],
    pub byte_orders: &'a [&'a str],
    pub limitations: &'a [&'a str],
    pub evidence: &'a [&'a str],
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct CapabilityReport<'a> {
    pub schema_version: u32,
    pub version: &'a str,
    /// This is implementation availability, not target runtime evidence.
    pub scope: &'a str,
    pub capabilities: &'a [Capability<'a>],
}

impl<'a> CapabilityReport<'a> {
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&'a Capability<'a>> {
        self.capabilities.iter().find(|entry| entry.id == id)
    }

    /// Distinguish a known but unavailable operation from an unknown identifier.
    pub fn require(&self, id: &str) -> Result<&'a Capability<'a>, LookupError> {
        match self.get(id) {
            Some(entry) if entry.status.is_usable() => Ok(entry),
            Some(_) => Err(LookupError::Unavailable),
            None => Err(LookupError::Absent),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LookupError {
    Absent,
    Unavailable,
}

impl fmt::Display for LookupError {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        output.write_str(match self {
            Self::Absent => "capability identifier is not registered",
            Self::Unavailable => "capability has no loaded production implementation",
        })
    }
}

impl std::error::Error for LookupError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegistrationError {
    InvalidDescriptor,
    Duplicate,
    Capacity,
}

impl fmt::Display for RegistrationError {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        output.write_str(match self {
            Self::InvalidDescriptor => "capability descriptor is invalid or exceeds a limit",
            Self::Duplicate => "capability identifier is already registered",
            Self::Capacity => "capability registry is full",
        })
    }
}

impl std::error::Error for RegistrationError {}

/// Bounded capability registration for a frontend or a future extension host.
///
/// The host must load and validate an actual backend before registering its
/// usable operations. Registration only stores descriptions; it never loads code,
/// changes the built-in report, edits a document, or grants file/device access.
/// The portable extension loader, ABI and permission contract remain issue #82.
#[derive(Debug, Default)]
pub struct Registry<'a> {
    entries: Vec<Capability<'a>>,
}

impl<'a> Registry<'a> {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a descriptor in stable identifier order without replacing a peer.
    pub fn register(&mut self, capability: Capability<'a>) -> Result<(), RegistrationError> {
        validate(&capability)?;
        let position = match self
            .entries
            .binary_search_by_key(&capability.id, |entry| entry.id)
        {
            Ok(_) => return Err(RegistrationError::Duplicate),
            Err(position) => position,
        };
        if self.entries.len() == MAX_CAPABILITIES {
            return Err(RegistrationError::Capacity);
        }
        self.entries.insert(position, capability);
        Ok(())
    }

    #[must_use]
    pub fn report(&self) -> CapabilityReport<'_> {
        report(&self.entries)
    }
}

/// Query the exact production catalog used by native and headless output.
///
/// The portable core also exposes this API to future browser adapters. It does
/// not imply that a browser frontend or every required runtime target exists.
#[must_use]
pub const fn current() -> CapabilityReport<'static> {
    report(BUILT_INS)
}

const fn report<'a>(capabilities: &'a [Capability<'a>]) -> CapabilityReport<'a> {
    CapabilityReport {
        schema_version: SCHEMA_VERSION,
        version: env!("CARGO_PKG_VERSION"),
        scope: "compiled implementation availability; target runtime evidence is separate",
        capabilities,
    }
}

fn valid_text(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_TEXT_BYTES && !value.chars().any(char::is_control)
}

fn validate(entry: &Capability<'_>) -> Result<(), RegistrationError> {
    let valid_id = !entry.id.is_empty()
        && entry.id.len() <= MAX_ID_BYTES
        && entry.id.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'-')
        })
        && entry.id.as_bytes()[0].is_ascii_lowercase()
        && !entry.id.ends_with('.')
        && !entry.id.ends_with('-')
        && !entry.id.contains("..");
    let valid_details = [
        entry.modes,
        entry.byte_orders,
        entry.limitations,
        entry.evidence,
    ]
    .into_iter()
    .all(|items| items.len() <= MAX_DETAIL_ITEMS && items.iter().all(|item| valid_text(item)));
    let valid_backend = match entry.backend {
        Some(backend) => valid_text(backend.name) && valid_text(backend.version),
        None => !entry.status.is_usable(),
    };
    let valid_scope = !entry.evidence.is_empty()
        && (entry.status == Status::Available || !entry.limitations.is_empty())
        && (entry.status.is_usable() || (entry.modes.is_empty() && entry.byte_orders.is_empty()));
    if valid_id && valid_details && valid_backend && valid_scope {
        Ok(())
    } else {
        Err(RegistrationError::InvalidDescriptor)
    }
}

const CORE: Backend<'static> = Backend {
    name: "hiewbie-core",
    version: env!("CARGO_PKG_VERSION"),
};
const X86: Backend<'static> = Backend {
    name: "iced-x86",
    version: "1.22.0",
};
const ARM: Backend<'static> = Backend {
    name: "yaxpeax-arm",
    version: "0.5.0",
};

const BUILT_INS: &[Capability<'static>] = &[
    Capability {
        id: "assemble.aarch64",
        category: Category::Assembly,
        status: Status::Unavailable,
        backend: None,
        modes: &[],
        byte_orders: &[],
        limitations: &["No production textual assembler; research probes do not enable editing."],
        evidence: &["docs/adr/0006-text-assembler-selection.md"],
    },
    Capability {
        id: "assemble.arm",
        category: Category::Assembly,
        status: Status::Unavailable,
        backend: None,
        modes: &[],
        byte_orders: &[],
        limitations: &["No production textual assembler; research probes do not enable editing."],
        evidence: &["docs/adr/0006-text-assembler-selection.md"],
    },
    Capability {
        id: "assemble.ebpf",
        category: Category::Assembly,
        status: Status::Unavailable,
        backend: None,
        modes: &[],
        byte_orders: &[],
        limitations: &[
            "Optional eBPF bytecode assembly is deferred; no execution or kernel loading is provided.",
        ],
        evidence: &["docs/capabilities.md"],
    },
    Capability {
        id: "assemble.mips32",
        category: Category::Assembly,
        status: Status::Unavailable,
        backend: None,
        modes: &[],
        byte_orders: &[],
        limitations: &["Textual assembly is evaluated independently and deferred."],
        evidence: &["docs/capabilities.md"],
    },
    Capability {
        id: "assemble.mips64",
        category: Category::Assembly,
        status: Status::Unavailable,
        backend: None,
        modes: &[],
        byte_orders: &[],
        limitations: &["Textual assembly is evaluated independently and deferred."],
        evidence: &["docs/capabilities.md"],
    },
    Capability {
        id: "assemble.powerpc32",
        category: Category::Assembly,
        status: Status::Unavailable,
        backend: None,
        modes: &[],
        byte_orders: &[],
        limitations: &["Textual assembly is evaluated independently and deferred."],
        evidence: &["docs/capabilities.md"],
    },
    Capability {
        id: "assemble.powerpc64",
        category: Category::Assembly,
        status: Status::Unavailable,
        backend: None,
        modes: &[],
        byte_orders: &[],
        limitations: &["Textual assembly is evaluated independently and deferred."],
        evidence: &["docs/capabilities.md"],
    },
    Capability {
        id: "assemble.riscv32",
        category: Category::Assembly,
        status: Status::Unavailable,
        backend: None,
        modes: &[],
        byte_orders: &[],
        limitations: &["Textual assembly is evaluated independently and deferred."],
        evidence: &["docs/capabilities.md"],
    },
    Capability {
        id: "assemble.riscv64",
        category: Category::Assembly,
        status: Status::Unavailable,
        backend: None,
        modes: &[],
        byte_orders: &[],
        limitations: &["Textual assembly is evaluated independently and deferred."],
        evidence: &["docs/capabilities.md"],
    },
    Capability {
        id: "assemble.thumb",
        category: Category::Assembly,
        status: Status::Unavailable,
        backend: None,
        modes: &[],
        byte_orders: &[],
        limitations: &["No production textual assembler; research probes do not enable editing."],
        evidence: &["docs/adr/0006-text-assembler-selection.md"],
    },
    Capability {
        id: "assemble.x86-16",
        category: Category::Assembly,
        status: Status::Unavailable,
        backend: None,
        modes: &[],
        byte_orders: &[],
        limitations: &["No production textual assembler; research probes do not enable editing."],
        evidence: &["docs/adr/0006-text-assembler-selection.md"],
    },
    Capability {
        id: "assemble.x86-32",
        category: Category::Assembly,
        status: Status::Unavailable,
        backend: None,
        modes: &[],
        byte_orders: &[],
        limitations: &["No production textual assembler; research probes do not enable editing."],
        evidence: &["docs/adr/0006-text-assembler-selection.md"],
    },
    Capability {
        id: "assemble.x86-64",
        category: Category::Assembly,
        status: Status::Unavailable,
        backend: None,
        modes: &[],
        byte_orders: &[],
        limitations: &["No production textual assembler; research probes do not enable editing."],
        evidence: &["docs/adr/0006-text-assembler-selection.md"],
    },
    Capability {
        id: "decode.aarch64",
        category: Category::Decode,
        status: Status::Limited,
        backend: Some(ARM),
        modes: &["aarch64"],
        byte_orders: &["little"],
        limitations: &[
            "Complete SVE/SVE2 and extension coverage is not qualified.",
            "Big-endian instructions are unsupported.",
        ],
        evidence: &[
            "crates/hiewbie-core/src/instructions/tests.rs",
            "docs/adr/0004-analysis-backends.md",
        ],
    },
    Capability {
        id: "decode.arm",
        category: Category::Decode,
        status: Status::Limited,
        backend: Some(ARM),
        modes: &["arm"],
        byte_orders: &["little"],
        limitations: &[
            "ARM revision filtering and Thumb IT-state propagation are incomplete.",
            "Big-endian instructions are unsupported.",
        ],
        evidence: &[
            "crates/hiewbie-core/src/instructions/tests.rs",
            "docs/adr/0004-analysis-backends.md",
        ],
    },
    Capability {
        id: "decode.ebpf",
        category: Category::Decode,
        status: Status::Unavailable,
        backend: None,
        modes: &[],
        byte_orders: &[],
        limitations: &[
            "Optional eBPF bytecode inspection is deferred; classic BPF is a separate mode.",
        ],
        evidence: &["docs/capabilities.md"],
    },
    Capability {
        id: "decode.mips32",
        category: Category::Decode,
        status: Status::Unavailable,
        backend: None,
        modes: &[],
        byte_orders: &[],
        limitations: &["Evaluated extension is deferred; no production decoder is loaded."],
        evidence: &["docs/capabilities.md"],
    },
    Capability {
        id: "decode.mips64",
        category: Category::Decode,
        status: Status::Unavailable,
        backend: None,
        modes: &[],
        byte_orders: &[],
        limitations: &["Evaluated extension is deferred; no production decoder is loaded."],
        evidence: &["docs/capabilities.md"],
    },
    Capability {
        id: "decode.powerpc32",
        category: Category::Decode,
        status: Status::Unavailable,
        backend: None,
        modes: &[],
        byte_orders: &[],
        limitations: &["Evaluated extension is deferred; no production decoder is loaded."],
        evidence: &["docs/capabilities.md"],
    },
    Capability {
        id: "decode.powerpc64",
        category: Category::Decode,
        status: Status::Unavailable,
        backend: None,
        modes: &[],
        byte_orders: &[],
        limitations: &["Evaluated extension is deferred; no production decoder is loaded."],
        evidence: &["docs/capabilities.md"],
    },
    Capability {
        id: "decode.riscv32",
        category: Category::Decode,
        status: Status::Unavailable,
        backend: None,
        modes: &[],
        byte_orders: &[],
        limitations: &["Evaluated extension is deferred; no production decoder is loaded."],
        evidence: &["docs/capabilities.md"],
    },
    Capability {
        id: "decode.riscv64",
        category: Category::Decode,
        status: Status::Unavailable,
        backend: None,
        modes: &[],
        byte_orders: &[],
        limitations: &["Evaluated extension is deferred; no production decoder is loaded."],
        evidence: &["docs/capabilities.md"],
    },
    Capability {
        id: "decode.thumb",
        category: Category::Decode,
        status: Status::Limited,
        backend: Some(ARM),
        modes: &["thumb"],
        byte_orders: &["little"],
        limitations: &[
            "ARM revision filtering and Thumb IT-state propagation are incomplete.",
            "Big-endian instructions are unsupported.",
        ],
        evidence: &[
            "crates/hiewbie-core/src/instructions/tests.rs",
            "docs/adr/0004-analysis-backends.md",
        ],
    },
    Capability {
        id: "decode.x86-16",
        category: Category::Decode,
        status: Status::Limited,
        backend: Some(X86),
        modes: &["x86-16"],
        byte_orders: &["little"],
        limitations: &[
            "Versioned extension coverage is incomplete; unknown encodings are not universally invalid.",
        ],
        evidence: &[
            "crates/hiewbie-core/src/instructions/tests.rs",
            "docs/adr/0004-analysis-backends.md",
        ],
    },
    Capability {
        id: "decode.x86-32",
        category: Category::Decode,
        status: Status::Limited,
        backend: Some(X86),
        modes: &["x86-32"],
        byte_orders: &["little"],
        limitations: &[
            "Versioned extension coverage is incomplete; unknown encodings are not universally invalid.",
        ],
        evidence: &[
            "crates/hiewbie-core/src/instructions/tests.rs",
            "docs/adr/0004-analysis-backends.md",
        ],
    },
    Capability {
        id: "decode.x86-64",
        category: Category::Decode,
        status: Status::Limited,
        backend: Some(X86),
        modes: &["x86-64"],
        byte_orders: &["little"],
        limitations: &[
            "Versioned extension coverage is incomplete; unknown encodings are not universally invalid.",
        ],
        evidence: &[
            "crates/hiewbie-core/src/instructions/tests.rs",
            "docs/adr/0004-analysis-backends.md",
        ],
    },
    Capability {
        id: "devices.raw",
        category: Category::Devices,
        status: Status::Unavailable,
        backend: None,
        modes: &[],
        byte_orders: &[],
        limitations: &[
            "No raw-device adapter or non-atomic device persistence; ordinary file access does not grant device access.",
        ],
        evidence: &["https://github.com/stokaro/hiewbie/issues/85"],
    },
    Capability {
        id: "extensions.hem",
        category: Category::Extensions,
        status: Status::Unavailable,
        backend: None,
        modes: &[],
        byte_orders: &[],
        limitations: &[
            "Original Windows HEM binary compatibility is not implemented and requires a separate decision.",
        ],
        evidence: &["https://github.com/stokaro/hiewbie/issues/84"],
    },
    Capability {
        id: "extensions.portable",
        category: Category::Extensions,
        status: Status::Unavailable,
        backend: None,
        modes: &[],
        byte_orders: &[],
        limitations: &[
            "Capability registration is descriptive; no portable extension loader, ABI or plugin execution exists.",
        ],
        evidence: &[
            "docs/capabilities.md",
            "https://github.com/stokaro/hiewbie/issues/82",
        ],
    },
    Capability {
        id: "format.ar",
        category: Category::Format,
        status: Status::Unavailable,
        backend: None,
        modes: &[],
        byte_orders: &[],
        limitations: &["Archive-member browsing is deferred; thin archive paths are not followed."],
        evidence: &["docs/capabilities.md"],
    },
    Capability {
        id: "format.elf",
        category: Category::Format,
        status: Status::Limited,
        backend: Some(CORE),
        modes: &["elf32", "elf64"],
        byte_orders: &["little", "big"],
        limitations: &[
            "Headers and PT_LOAD mappings; extended numbering and relocation browsing are not implemented.",
        ],
        evidence: &["crates/hiewbie-core/src/formats/tests.rs"],
    },
    Capability {
        id: "format.fat-macho",
        category: Category::Format,
        status: Status::Limited,
        backend: Some(CORE),
        modes: &["fat32", "fat64"],
        byte_orders: &["little", "big"],
        limitations: &[
            "Slice selection and checked global mappings; selected members must be supported thin Mach-O images.",
        ],
        evidence: &["crates/hiewbie-core/src/formats/tests.rs"],
    },
    Capability {
        id: "format.le",
        category: Category::Format,
        status: Status::Unavailable,
        backend: None,
        modes: &[],
        byte_orders: &[],
        limitations: &[
            "Mandatory legacy format interpretation remains unimplemented; raw byte access is available.",
        ],
        evidence: &["docs/compatibility-status.md"],
    },
    Capability {
        id: "format.lx",
        category: Category::Format,
        status: Status::Unavailable,
        backend: None,
        modes: &[],
        byte_orders: &[],
        limitations: &[
            "Mandatory legacy format interpretation remains unimplemented; raw byte access is available.",
        ],
        evidence: &["docs/compatibility-status.md"],
    },
    Capability {
        id: "format.macho",
        category: Category::Format,
        status: Status::Limited,
        backend: Some(CORE),
        modes: &["macho32", "macho64"],
        byte_orders: &["little", "big"],
        limitations: &[
            "Thin headers and segment mappings; complete load-command and relocation navigation remain open.",
        ],
        evidence: &["crates/hiewbie-core/src/formats/tests.rs"],
    },
    Capability {
        id: "format.mz",
        category: Category::Format,
        status: Status::Unavailable,
        backend: None,
        modes: &[],
        byte_orders: &[],
        limitations: &[
            "Mandatory legacy format interpretation remains unimplemented; raw byte access is available.",
        ],
        evidence: &["docs/compatibility-status.md"],
    },
    Capability {
        id: "format.ne",
        category: Category::Format,
        status: Status::Unavailable,
        backend: None,
        modes: &[],
        byte_orders: &[],
        limitations: &[
            "Mandatory legacy format interpretation remains unimplemented; raw byte access is available.",
        ],
        evidence: &["docs/compatibility-status.md"],
    },
    Capability {
        id: "format.netware",
        category: Category::Format,
        status: Status::Unavailable,
        backend: None,
        modes: &[],
        byte_orders: &[],
        limitations: &[
            "Mandatory legacy format interpretation remains unimplemented; raw byte access is available.",
        ],
        evidence: &["docs/compatibility-status.md"],
    },
    Capability {
        id: "format.pe",
        category: Category::Format,
        status: Status::Limited,
        backend: Some(CORE),
        modes: &["pe32", "pe32+"],
        byte_orders: &["little"],
        limitations: &[
            "Headers and load mappings; remaining directory structures, resources and relocations are not implemented.",
        ],
        evidence: &["crates/hiewbie-core/src/formats/tests.rs"],
    },
    Capability {
        id: "format.raw",
        category: Category::Format,
        status: Status::Available,
        backend: Some(CORE),
        modes: &[],
        byte_orders: &[],
        limitations: &[],
        evidence: &["crates/hiewbie-core/src/analysis_tests.rs"],
    },
    Capability {
        id: "format.te",
        category: Category::Format,
        status: Status::Unavailable,
        backend: None,
        modes: &[],
        byte_orders: &[],
        limitations: &[
            "Mandatory legacy format interpretation remains unimplemented; raw byte access is available.",
        ],
        evidence: &["docs/compatibility-status.md"],
    },
    Capability {
        id: "format.wasm",
        category: Category::Format,
        status: Status::Unavailable,
        backend: None,
        modes: &[],
        byte_orders: &[],
        limitations: &[
            "WebAssembly module inspection is deferred and independent of compiling hiewbie for WASM.",
        ],
        evidence: &["docs/capabilities.md"],
    },
    Capability {
        id: "raw.overwrite",
        category: Category::Raw,
        status: Status::Available,
        backend: Some(CORE),
        modes: &[],
        byte_orders: &[],
        limitations: &[],
        evidence: &[
            "crates/hiewbie-core/src/tests.rs",
            "crates/hiewbie/tests/persistence.rs",
        ],
    },
    Capability {
        id: "raw.read",
        category: Category::Raw,
        status: Status::Available,
        backend: Some(CORE),
        modes: &[],
        byte_orders: &[],
        limitations: &[],
        evidence: &["crates/hiewbie-core/src/tests.rs"],
    },
    Capability {
        id: "references.aarch64",
        category: Category::References,
        status: Status::Unavailable,
        backend: None,
        modes: &[],
        byte_orders: &[],
        limitations: &[
            "Direct branch following is available through decoding; reference search is not implemented.",
        ],
        evidence: &["crates/hiewbie-core/src/references.rs"],
    },
    Capability {
        id: "references.arm",
        category: Category::References,
        status: Status::Unavailable,
        backend: None,
        modes: &[],
        byte_orders: &[],
        limitations: &[
            "Direct branch following is available through decoding; reference search is not implemented.",
        ],
        evidence: &["crates/hiewbie-core/src/references.rs"],
    },
    Capability {
        id: "references.thumb",
        category: Category::References,
        status: Status::Unavailable,
        backend: None,
        modes: &[],
        byte_orders: &[],
        limitations: &[
            "Direct branch following is available through decoding; reference search is not implemented.",
        ],
        evidence: &["crates/hiewbie-core/src/references.rs"],
    },
    Capability {
        id: "references.x86",
        category: Category::References,
        status: Status::Limited,
        backend: Some(X86),
        modes: &["x86-16", "x86-32", "x86-64"],
        byte_orders: &["little"],
        limitations: &[
            "Bounded near-branch, selected mapped-memory and literal-operand search; no general control-flow or loader resolution.",
        ],
        evidence: &[
            "crates/hiewbie-core/src/references.rs",
            "crates/hiewbie-core/src/analysis_tests.rs",
        ],
    },
    Capability {
        id: "symbols.elf",
        category: Category::Symbols,
        status: Status::Limited,
        backend: Some(CORE),
        modes: &[],
        byte_orders: &[],
        limitations: &[
            "Section-backed SYMTAB/DYNSYM only; sectionless dynamic symbols, versioning and providers remain open.",
        ],
        evidence: &[
            "crates/hiewbie-core/src/symbols",
            "docs/adr/0005-symbol-navigation.md",
        ],
    },
    Capability {
        id: "symbols.macho",
        category: Category::Symbols,
        status: Status::Limited,
        backend: Some(CORE),
        modes: &[],
        byte_orders: &[],
        limitations: &[
            "LC_SYMTAB/nlist only; export tries, dyld bindings and chained fixups remain open.",
        ],
        evidence: &[
            "crates/hiewbie-core/src/symbols",
            "docs/adr/0005-symbol-navigation.md",
        ],
    },
    Capability {
        id: "symbols.pe",
        category: Category::Symbols,
        status: Status::Limited,
        backend: Some(CORE),
        modes: &[],
        byte_orders: &[],
        limitations: &[
            "Ordinary imports and exports only; delay/bound imports, COFF/debug and loader resolution remain open.",
        ],
        evidence: &[
            "crates/hiewbie-core/src/symbols",
            "docs/adr/0005-symbol-navigation.md",
        ],
    },
];

#[cfg(test)]
#[path = "capabilities_tests.rs"]
mod tests;
