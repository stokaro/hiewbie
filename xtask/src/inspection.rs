//! Artifact metadata inspection never executes the supplied binary.
use std::collections::BTreeSet;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::Result;
use crate::manifest::{BinaryFormat, System, Target, version, version_exceeds};

const MAX_TOOL_OUTPUT: usize = 2 * 1024 * 1024;
const TOOL_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ArtifactIdentity {
    pub target: String,
    pub source_commit: String,
    pub sha256: String,
    pub size: u64,
}

impl ArtifactIdentity {
    pub fn from_file(path: &Path, target: &str, commit: &str) -> Result<Self> {
        if !hex_identifier(commit, 40) {
            return Err("source commit must be a full 40-character lowercase Git SHA".into());
        }
        let mut file = open_artifact(path)?;
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err("artifact must be a regular file".into());
        }
        let mut digest = Sha256::new();
        let mut buffer = [0; 64 * 1024];
        let mut size = 0_u64;
        loop {
            let count = file.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            digest.update(&buffer[..count]);
            size = size
                .checked_add(count as u64)
                .ok_or("artifact length overflow")?;
        }
        if size != metadata.len() || size != file.metadata()?.len() {
            return Err("artifact changed while being hashed".into());
        }
        Ok(Self {
            target: target.into(),
            source_commit: commit.into(),
            sha256: format!("{:x}", digest.finalize()),
            size,
        })
    }

    pub fn validate(&self) -> Result<()> {
        if self.target.is_empty()
            || !hex_identifier(&self.source_commit, 40)
            || !hex_identifier(&self.sha256, 64)
            || self.size == 0
        {
            return Err(
                "invalid artifact identity: target, full commit, SHA-256 and size are required"
                    .into(),
            );
        }
        Ok(())
    }
}

fn open_artifact(path: &Path) -> Result<File> {
    if !std::fs::metadata(path)?.is_file() {
        return Err("artifact must be a regular file".into());
    }
    let mut options = OpenOptions::new();
    options.read(true);
    // Avoid a blocking FIFO open if a regular path is replaced after metadata.
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK);
    }
    let file = options.open(path)?;
    if !file.metadata()?.is_file() {
        return Err("artifact was replaced with a non-regular file".into());
    }
    Ok(file)
}

pub fn hex_identifier(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum InspectionStatus {
    Complete,
    Unsupported,
    Mismatch,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum BaselineVerdict {
    CompatibleObservedMetadata,
    Exceeds,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BinaryMetadata {
    pub format: BinaryFormat,
    pub machine: String,
    pub byte_order: String,
    pub executable_kind: String,
    pub macho_platforms: Vec<u32>,
    pub conflicting_deployment_commands: bool,
    pub elf_osabi: Option<u8>,
    pub interpreter: Option<String>,
    pub libraries: Vec<String>,
    pub glibc_versions: Vec<String>,
    pub macos_minimum: Option<String>,
    pub pe_subsystem_version: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ToolEvidence {
    pub program: String,
    pub arguments: Vec<String>,
    pub output_sha256: String,
    pub output: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InspectionReport {
    pub schema_version: u32,
    pub identity: ArtifactIdentity,
    pub baseline_id: String,
    pub status: InspectionStatus,
    pub baseline_verdict: BaselineVerdict,
    pub metadata: Option<BinaryMetadata>,
    pub tool: Option<ToolEvidence>,
    pub diagnostics: Vec<String>,
    pub scope: String,
}

pub fn inspect(path: &Path, target: &Target, commit: &str) -> Result<InspectionReport> {
    let identity = ArtifactIdentity::from_file(path, &target.triple, commit)?;
    identity.validate()?;
    let mut report = InspectionReport {
        schema_version: 1, identity, baseline_id: target.runtime.id.clone(),
        status: InspectionStatus::Unsupported, baseline_verdict: BaselineVerdict::Unknown,
        metadata: None, tool: None, diagnostics: Vec::new(),
        scope: "Metadata only: source commit is declared by the build producer; this inspection binds it to exact artifact bytes, but does not prove source provenance, runtime behavior, compiler family, packaging or release completeness.".into(),
    };
    let mut file = open_artifact(path)?;
    let metadata = match read_header(&mut file, report.identity.size) {
        Ok(metadata) => metadata,
        Err(error) => {
            report.diagnostics.push(error.to_string());
            return Ok(report);
        }
    };
    if metadata.format != target.runtime.binary_format
        || metadata.machine != target.runtime.machine
        || metadata.byte_order != "little"
    {
        report.status = InspectionStatus::Mismatch;
        report
            .diagnostics
            .push("binary format, machine or byte order does not match the declared target".into());
        report.metadata = Some(metadata);
        return Ok(report);
    }
    let candidates = metadata_commands(metadata.format);
    report.metadata = Some(metadata);
    for (program, flags) in candidates {
        match run_tool(program, flags, path) {
            Ok(output) => {
                report.tool = Some(ToolEvidence {
                    program: (*program).into(),
                    arguments: flags.iter().map(|value| (*value).into()).collect(),
                    output_sha256: format!("{:x}", Sha256::digest(output.as_bytes())),
                    output,
                });
                break;
            }
            Err(error) => report.diagnostics.push(format!("{program}: {error}")),
        }
    }
    if let Some(tool) = &report.tool {
        let metadata = report.metadata.as_mut().expect("header was inspected");
        parse_metadata(metadata, &tool.output);
        let (status, verdict, details) = assess(metadata, target);
        report.status = status;
        report.baseline_verdict = verdict;
        report.diagnostics.extend(details);
    } else {
        report.diagnostics.push(if candidates.is_empty() {
            "Wasm header is recognized, but feature/import inspection and real browser baseline verification remain unsupported by this native metadata hook.".into()
        } else {
            "No supported metadata tool completed successfully; header recognition alone is not an ABI inspection.".into()
        });
    }
    if ArtifactIdentity::from_file(path, &target.triple, commit)? != report.identity {
        return Err("artifact changed during inspection".into());
    }
    Ok(report)
}

fn metadata_commands(format: BinaryFormat) -> &'static [(&'static str, &'static [&'static str])] {
    match format {
        BinaryFormat::Elf => &[
            (
                "readelf",
                &[
                    "--wide",
                    "--file-header",
                    "--program-headers",
                    "--dynamic",
                    "--version-info",
                    "--notes",
                ],
            ),
            (
                "llvm-readelf",
                &[
                    "--wide",
                    "--file-header",
                    "--program-headers",
                    "--dynamic",
                    "--version-info",
                    "--notes",
                ],
            ),
        ],
        BinaryFormat::Macho => &[("otool", &["-l"])],
        BinaryFormat::Pe => &[("llvm-readobj", &["--file-headers", "--coff-imports"])],
        BinaryFormat::Wasm => &[],
    }
}

pub fn validate_recorded_metadata(report: &InspectionReport) -> Result<()> {
    let metadata = report
        .metadata
        .as_ref()
        .ok_or("inspection lacks metadata")?;
    let tool = report
        .tool
        .as_ref()
        .ok_or("inspection lacks a metadata producer")?;
    if tool.output.len() > MAX_TOOL_OUTPUT
        || !metadata_commands(metadata.format)
            .iter()
            .any(|(program, flags)| {
                *program == tool.program
                    && flags
                        .iter()
                        .copied()
                        .eq(tool.arguments.iter().map(String::as_str))
            })
    {
        return Err("unknown metadata tool/arguments or excessive output".into());
    }
    let mut replayed = metadata.clone();
    replayed.interpreter = None;
    replayed.libraries.clear();
    replayed.glibc_versions.clear();
    replayed.macos_minimum = None;
    replayed.pe_subsystem_version = None;
    replayed.macho_platforms.clear();
    replayed.conflicting_deployment_commands = false;
    parse_metadata(&mut replayed, &tool.output);
    if &replayed != metadata {
        return Err("structured ABI fields disagree with recorded metadata-tool output".into());
    }
    Ok(())
}

pub fn validate_actual_header(path: &Path, report: &InspectionReport) -> Result<()> {
    let mut file = open_artifact(path)?;
    let length = file.metadata()?.len();
    let actual = read_header(&mut file, length)?;
    let claimed = report
        .metadata
        .as_ref()
        .ok_or("inspection lacks metadata")?;
    if actual.format != claimed.format
        || actual.machine != claimed.machine
        || actual.byte_order != claimed.byte_order
        || actual.elf_osabi != claimed.elf_osabi
        || actual.executable_kind != claimed.executable_kind
    {
        return Err("recorded binary header does not match the actual artifact".into());
    }
    Ok(())
}

fn read_at<R: Read + Seek>(
    reader: &mut R,
    length: u64,
    offset: u64,
    count: usize,
) -> Result<Vec<u8>> {
    if offset
        .checked_add(count as u64)
        .is_none_or(|end| end > length)
    {
        return Err("truncated binary header".into());
    }
    reader.seek(SeekFrom::Start(offset))?;
    let mut bytes = vec![0; count];
    reader.read_exact(&mut bytes)?;
    Ok(bytes)
}

fn read_header<R: Read + Seek>(reader: &mut R, length: u64) -> Result<BinaryMetadata> {
    let prefix = read_at(reader, length, 0, usize::try_from(length.min(64))?)?;
    let mut result = BinaryMetadata {
        format: BinaryFormat::Elf,
        machine: String::new(),
        byte_order: "little".into(),
        executable_kind: String::new(),
        macho_platforms: Vec::new(),
        conflicting_deployment_commands: false,
        elf_osabi: None,
        interpreter: None,
        libraries: Vec::new(),
        glibc_versions: Vec::new(),
        macos_minimum: None,
        pe_subsystem_version: None,
    };
    if prefix.starts_with(b"\x7fELF") {
        if prefix.len() < 64 || prefix[4] != 2 || !matches!(prefix[5], 1 | 2) {
            return Err(
                "only complete ELF64 headers are supported for the selected release targets".into(),
            );
        }
        let machine = if prefix[5] == 1 {
            u16::from_le_bytes([prefix[18], prefix[19]])
        } else {
            result.byte_order = "big".into();
            u16::from_be_bytes([prefix[18], prefix[19]])
        };
        result.machine = match machine {
            62 => "x86_64",
            183 => "aarch64",
            243 => "riscv64",
            _ => "unknown",
        }
        .into();
        result.elf_osabi = Some(prefix[7]);
        let kind = if prefix[5] == 1 {
            u16::from_le_bytes([prefix[16], prefix[17]])
        } else {
            u16::from_be_bytes([prefix[16], prefix[17]])
        };
        result.executable_kind = match kind {
            2 => "executable",
            3 => "dynamic",
            _ => "other",
        }
        .into();
    } else if prefix.starts_with(&[0xCF, 0xFA, 0xED, 0xFE]) {
        if prefix.len() < 32 {
            return Err("truncated Mach-O header".into());
        }
        result.format = BinaryFormat::Macho;
        result.executable_kind = if u32::from_le_bytes(prefix[12..16].try_into()?) == 2 {
            "executable"
        } else {
            "other"
        }
        .into();
        result.machine = match u32::from_le_bytes(prefix[4..8].try_into()?) {
            0x0100_0007 => "x86_64",
            0x0100_000C => "aarch64",
            _ => "unknown",
        }
        .into();
    } else if prefix.starts_with(b"MZ") {
        if prefix.len() < 64 {
            return Err("truncated DOS header".into());
        }
        let offset = u32::from_le_bytes(prefix[60..64].try_into()?) as u64;
        let coff = read_at(reader, length, offset, 24)?;
        if !coff.starts_with(b"PE\0\0") {
            return Err("missing PE signature".into());
        }
        result.format = BinaryFormat::Pe;
        let flags = u16::from_le_bytes([coff[22], coff[23]]);
        result.executable_kind = if flags & 0x2002 == 2 {
            "executable"
        } else {
            "other"
        }
        .into();
        let magic = read_at(reader, length, offset + 24, 2)?;
        if magic != [0x0B, 0x02] {
            return Err("selected Windows targets require PE32+".into());
        }
        result.machine = match u16::from_le_bytes([coff[4], coff[5]]) {
            0x8664 => "x86_64",
            0xAA64 => "aarch64",
            _ => "unknown",
        }
        .into();
    } else if prefix.starts_with(b"\0asm\x01\0\0\0") {
        result.format = BinaryFormat::Wasm;
        result.machine = "wasm32".into();
        result.executable_kind = "module".into();
    } else {
        return Err("unsupported binary header (universal Mach-O requires both slice inspections under #94)".into());
    }
    Ok(result)
}

fn run_tool(program: &str, flags: &[&str], artifact: &Path) -> Result<String> {
    run_tool_limited(program, flags, artifact, TOOL_TIMEOUT)
}

fn run_tool_limited(
    program: &str,
    flags: &[&str],
    artifact: &Path,
    timeout: Duration,
) -> Result<String> {
    // Canonical absolute paths cannot be interpreted as command-line options.
    let artifact = artifact.canonicalize()?;
    let child = Command::new(program)
        .args(flags)
        .arg(artifact)
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut child = ChildGuard(child);
    let stdout = child.0.stdout.take().ok_or("missing tool output pipe")?;
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = stdout
            .take((MAX_TOOL_OUTPUT + 1) as u64)
            .read_to_end(&mut bytes)
            .map(|_| bytes);
        let _ = sender.send(result);
    });
    let deadline = Instant::now() + timeout;
    let mut output = None;
    let mut status = None;
    loop {
        if output.is_none() {
            match receiver.try_recv() {
                Ok(result) => output = Some(result?),
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    return Err("metadata reader stopped unexpectedly".into());
                }
            }
        }
        if output
            .as_ref()
            .is_some_and(|bytes| bytes.len() > MAX_TOOL_OUTPUT)
            || Instant::now() >= deadline
        {
            return Err("metadata tool exceeded its 20-second / 2-MiB output budget".into());
        }
        if status.is_none() {
            status = child.0.try_wait()?;
        }
        if let (Some(bytes), Some(status)) = (&output, status) {
            if !status.success() {
                return Err(format!("metadata tool failed with {status}").into());
            }
            return String::from_utf8(bytes.clone()).map_err(Into::into);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

struct ChildGuard(std::process::Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

fn parse_metadata(metadata: &mut BinaryMetadata, output: &str) {
    let mut libraries = BTreeSet::new();
    let mut glibc = BTreeSet::new();
    let mut command = "";
    let mut subsystem_major = None;
    let mut subsystem_minor = None;
    for line in output.lines().map(str::trim) {
        match metadata.format {
            BinaryFormat::Elf => {
                if let Some((_, tail)) = line.split_once("Requesting program interpreter: ") {
                    metadata.interpreter = tail.strip_suffix(']').map(str::to_owned);
                }
                if line.contains("(NEEDED)")
                    && let Some((_, tail)) = line.split_once("Shared library: [")
                    && let Some(name) = tail.strip_suffix(']')
                {
                    libraries.insert(name.to_owned());
                }
                if let Some((_, tail)) = line.split_once("Name: GLIBC_")
                    && let Some(value) = tail.split_whitespace().next()
                {
                    glibc.insert(value.to_owned());
                }
            }
            BinaryFormat::Macho => {
                if let Some(value) = line.strip_prefix("cmd ") {
                    command = value;
                    match value {
                        "LC_VERSION_MIN_MACOSX" => metadata.macho_platforms.push(1),
                        "LC_VERSION_MIN_IPHONEOS" => metadata.macho_platforms.push(2),
                        "LC_VERSION_MIN_TVOS" => metadata.macho_platforms.push(3),
                        "LC_VERSION_MIN_WATCHOS" => metadata.macho_platforms.push(4),
                        _ => {}
                    }
                }
                if command == "LC_BUILD_VERSION"
                    && let Some(value) = line.strip_prefix("platform ")
                {
                    let platform = if value == "MACOS" {
                        1
                    } else {
                        value.parse().unwrap_or(0)
                    };
                    metadata.macho_platforms.push(platform);
                }
                // LC_BUILD_VERSION also contains tool/version records. Only
                // minos is its deployment floor; version is the floor only in
                // the legacy LC_VERSION_MIN_MACOSX command.
                let minimum = match command {
                    "LC_BUILD_VERSION" => line.strip_prefix("minos "),
                    "LC_VERSION_MIN_MACOSX" => line.strip_prefix("version "),
                    _ => None,
                };
                if let Some(value) = minimum
                    && version(value).is_some()
                {
                    if metadata
                        .macos_minimum
                        .as_ref()
                        .is_some_and(|prior| prior != value)
                    {
                        metadata.conflicting_deployment_commands = true;
                    }
                    metadata.macos_minimum = Some(value.into());
                }
                if matches!(
                    command,
                    "LC_LOAD_DYLIB" | "LC_LOAD_WEAK_DYLIB" | "LC_REEXPORT_DYLIB"
                ) && let Some(value) = line
                    .strip_prefix("name ")
                    .and_then(|value| value.split(" (offset ").next())
                {
                    libraries.insert(value.to_owned());
                }
            }
            BinaryFormat::Pe => {
                if let Some(value) = line.strip_prefix("MajorSubsystemVersion: ") {
                    subsystem_major = value.parse::<u32>().ok();
                }
                if let Some(value) = line.strip_prefix("MinorSubsystemVersion: ") {
                    subsystem_minor = value.parse::<u32>().ok();
                }
                if let Some(value) = line.strip_prefix("Name: ")
                    && value.to_ascii_lowercase().ends_with(".dll")
                {
                    libraries.insert(value.to_owned());
                }
            }
            BinaryFormat::Wasm => {}
        }
    }
    metadata.libraries = libraries.into_iter().collect();
    metadata.glibc_versions = glibc.into_iter().collect();
    metadata.pe_subsystem_version = subsystem_major
        .zip(subsystem_minor)
        .map(|(major, minor)| format!("{major}.{minor}"));
}

pub fn assess(
    metadata: &BinaryMetadata,
    target: &Target,
) -> (InspectionStatus, BaselineVerdict, Vec<String>) {
    use BaselineVerdict::*;
    use InspectionStatus::*;
    if metadata.format != target.runtime.binary_format
        || metadata.machine != target.runtime.machine
        || metadata.byte_order != "little"
    {
        return (
            Mismatch,
            Unknown,
            vec!["artifact identity does not match target format/machine/order".into()],
        );
    }
    if !matches!(
        metadata.executable_kind.as_str(),
        "executable" | "dynamic" | "module"
    ) {
        return (
            Mismatch,
            Unknown,
            vec!["artifact is not an executable for the selected CLI target".into()],
        );
    }
    match target.runtime.system {
        System::Linux => {
            let loader = metadata.interpreter.as_deref().unwrap_or_default();
            if metadata.elf_osabi == Some(9) || loader.contains("/ld-elf") {
                return (
                    Mismatch,
                    Unknown,
                    vec!["FreeBSD ELF cannot qualify a GNU Linux artifact".into()],
                );
            }
            if !loader.contains("ld-linux") || metadata.glibc_versions.is_empty() {
                return (Unsupported, Unknown, vec!["GNU loader and GLIBC version requirements were not both recovered; static/sectionless/other-libc ABI qualification is unsupported.".into()]);
            }
            let selected = &target.runtime.minimum_versions["glibc"];
            if metadata
                .glibc_versions
                .iter()
                .any(|value| version(value).is_none())
            {
                return (
                    Complete,
                    Unknown,
                    vec![
                        "Non-numeric GLIBC ABI requirements require explicit platform review."
                            .into(),
                    ],
                );
            }
            if metadata
                .glibc_versions
                .iter()
                .any(|value| version_exceeds(value, selected))
            {
                return (
                    Complete,
                    Exceeds,
                    vec![format!(
                        "artifact requires a GLIBC version above selected {selected}"
                    )],
                );
            }
            (Complete, CompatibleObservedMetadata, vec!["Observed GLIBC requirements fit the selected floor. Kernel/API availability and execution on the minimum runtime remain unverified.".into()])
        }
        System::Freebsd => {
            if metadata.elf_osabi != Some(9)
                || !metadata
                    .interpreter
                    .as_deref()
                    .is_some_and(|value| value.contains("/ld-elf"))
            {
                return (Mismatch, Unknown, vec!["FreeBSD OSABI and loader evidence are required; generic ELF machine equality is insufficient.".into()]);
            }
            (Complete, Unknown, vec!["FreeBSD loader/OSABI and dependencies inspected. Minimum-release syscall/library compatibility requires the selected guest runtime test.".into()])
        }
        System::Macos => {
            if metadata.macho_platforms.is_empty()
                || metadata
                    .macho_platforms
                    .iter()
                    .any(|platform| *platform != 1)
                || metadata.conflicting_deployment_commands
            {
                return (Mismatch, Unknown, vec!["Mach-O must have consistent macOS deployment commands; iOS/tvOS/watchOS binaries cannot qualify".into()]);
            }
            let Some(minimum) = &metadata.macos_minimum else {
                return (
                    Unsupported,
                    Unknown,
                    vec!["No supported macOS minimum-version load command was recovered.".into()],
                );
            };
            let selected = &target.runtime.minimum_versions["os"];
            if version_exceeds(minimum, selected) {
                (
                    Complete,
                    Exceeds,
                    vec![format!(
                        "Mach-O deployment target {minimum} exceeds selected macOS {selected}"
                    )],
                )
            } else {
                (Complete, CompatibleObservedMetadata, vec!["Mach-O deployment version and dylib load commands inspected; execution on the selected minimum remains pending.".into()])
            }
        }
        System::Windows => {
            if metadata.pe_subsystem_version.is_none() {
                return (
                    Unsupported,
                    Unknown,
                    vec!["PE optional-header subsystem metadata was not recovered.".into()],
                );
            }
            (Complete, Unknown, vec!["PE machine, subsystem version and DLL imports inspected. These do not prove MSVC build provenance or Windows API availability; build and minimum-runtime evidence remain required.".into()])
        }
        System::Browser => (
            Unsupported,
            Unknown,
            vec!["Native metadata inspection cannot verify browser feature support.".into()],
        ),
    }
}

#[cfg(test)]
pub(crate) mod tests;
