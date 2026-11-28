use super::*;
use crate::manifest::Manifest;
use std::io::Cursor;

fn target(triple: &str) -> Target {
    let manifest: Manifest =
        serde_json::from_str(include_str!("../../../build/targets.json")).unwrap();
    manifest.target(triple).unwrap().clone()
}

fn elf(machine: u16, osabi: u8) -> BinaryMetadata {
    let mut bytes = vec![0; 64];
    bytes[..4].copy_from_slice(b"\x7fELF");
    bytes[4] = 2;
    bytes[5] = 1;
    bytes[7] = osabi;
    bytes[16..18].copy_from_slice(&3_u16.to_le_bytes());
    bytes[18..20].copy_from_slice(&machine.to_le_bytes());
    read_header(&mut Cursor::new(bytes), 64).unwrap()
}

fn macho() -> BinaryMetadata {
    let mut bytes = vec![0; 32];
    bytes[..4].copy_from_slice(&[0xCF, 0xFA, 0xED, 0xFE]);
    bytes[4..8].copy_from_slice(&0x0100_000C_u32.to_le_bytes());
    bytes[12..16].copy_from_slice(&2_u32.to_le_bytes());
    read_header(&mut Cursor::new(bytes), 32).unwrap()
}

pub(crate) fn linux_report() -> InspectionReport {
    let target = target("x86_64-unknown-linux-gnu");
    let output = "[Requesting program interpreter: /lib64/ld-linux-x86-64.so.2]\n 0x1 (NEEDED) Shared library: [libc.so.6]\n 0x1 (NEEDED) Shared library: [libgcc_s.so.1]\n Name: GLIBC_2.17 Flags: none\n Name: GLIBC_2.34 Flags: none\n";
    let mut metadata = elf(62, 0);
    parse_metadata(&mut metadata, output);
    let (status, baseline_verdict, diagnostics) = assess(&metadata, &target);
    InspectionReport {
        schema_version: 1,
        identity: ArtifactIdentity {
            target: target.triple.clone(),
            source_commit: "a".repeat(40),
            sha256: "b".repeat(64),
            size: 4096,
        },
        baseline_id: target.runtime.id,
        status,
        baseline_verdict,
        metadata: Some(metadata),
        tool: Some(ToolEvidence {
            program: "readelf".into(),
            arguments: metadata_commands(BinaryFormat::Elf)[0]
                .1
                .iter()
                .map(|value| (*value).into())
                .collect(),
            output_sha256: format!("{:x}", Sha256::digest(output.as_bytes())),
            output: output.into(),
        }),
        diagnostics,
        scope: "Generated metadata fixture, never runtime evidence.".into(),
    }
}

#[test]
fn elf_machine_os_and_glibc_are_checked_independently() {
    let target = target("x86_64-unknown-linux-gnu");
    let mut report = linux_report();
    assert_eq!(report.status, InspectionStatus::Complete);
    assert_eq!(
        report.baseline_verdict,
        BaselineVerdict::CompatibleObservedMetadata
    );
    let metadata = report.metadata.as_mut().unwrap();
    metadata.glibc_versions.push("2.40".into());
    assert_eq!(assess(metadata, &target).1, BaselineVerdict::Exceeds);
    metadata.glibc_versions = vec!["ABI_DT_RELR".into()];
    assert_eq!(assess(metadata, &target).1, BaselineVerdict::Unknown);
    metadata.elf_osabi = Some(9);
    assert_eq!(assess(metadata, &target).0, InspectionStatus::Mismatch);
    let freebsd = super::tests::target("x86_64-unknown-freebsd");
    assert_eq!(assess(&elf(62, 0), &freebsd).0, InspectionStatus::Mismatch);
    let mut native = elf(62, 9);
    native.interpreter = Some("/libexec/ld-elf.so.1".into());
    assert_eq!(assess(&native, &freebsd).0, InspectionStatus::Complete);
    native.machine = "aarch64".into();
    assert_eq!(assess(&native, &freebsd).0, InspectionStatus::Mismatch);
}

#[test]
fn macos_platform_deployment_and_executable_type_cannot_be_substituted() {
    let target = target("aarch64-apple-darwin");
    let mut metadata = macho();
    parse_metadata(
        &mut metadata,
        "cmd LC_BUILD_VERSION\nplatform 1\nminos 11.0\ncmd LC_LOAD_DYLIB\nname /usr/lib/libSystem.B.dylib (offset 24)\n",
    );
    assert_eq!(metadata.libraries, ["/usr/lib/libSystem.B.dylib"]);
    assert_eq!(
        assess(&metadata, &target).1,
        BaselineVerdict::CompatibleObservedMetadata
    );
    let mut ios = macho();
    parse_metadata(&mut ios, "cmd LC_BUILD_VERSION\nplatform 2\nminos 11.0\n");
    assert_eq!(assess(&ios, &target).0, InspectionStatus::Mismatch);
    let mut mixed = macho();
    parse_metadata(
        &mut mixed,
        "cmd LC_BUILD_VERSION\nplatform 1\nminos 11.0\ncmd LC_VERSION_MIN_MACOSX\nversion 12.0\n",
    );
    assert_eq!(assess(&mixed, &target).0, InspectionStatus::Mismatch);
    let mut higher = macho();
    parse_metadata(&mut higher, "cmd LC_VERSION_MIN_MACOSX\nversion 12.0\n");
    assert_eq!(assess(&higher, &target).1, BaselineVerdict::Exceeds);
    metadata.executable_kind = "other".into();
    assert_eq!(assess(&metadata, &target).0, InspectionStatus::Mismatch);
}

#[test]
fn pe_header_imports_and_minimum_runtime_limits_are_explicit() {
    let mut bytes = vec![0; 256];
    bytes[..2].copy_from_slice(b"MZ");
    bytes[60..64].copy_from_slice(&128_u32.to_le_bytes());
    bytes[128..132].copy_from_slice(b"PE\0\0");
    bytes[132..134].copy_from_slice(&0x8664_u16.to_le_bytes());
    bytes[150..152].copy_from_slice(&2_u16.to_le_bytes());
    bytes[152..154].copy_from_slice(&0x020B_u16.to_le_bytes());
    let mut metadata = read_header(&mut Cursor::new(bytes.clone()), 256).unwrap();
    parse_metadata(
        &mut metadata,
        "MajorSubsystemVersion: 6\nMinorSubsystemVersion: 0\nImport {\nName: KERNEL32.dll\n}\n",
    );
    assert_eq!(metadata.libraries, ["KERNEL32.dll"]);
    assert_eq!(metadata.pe_subsystem_version.as_deref(), Some("6.0"));
    assert_eq!(
        assess(&metadata, &target("x86_64-pc-windows-msvc")).1,
        BaselineVerdict::Unknown
    );
    bytes[150..152].copy_from_slice(&0x2002_u16.to_le_bytes());
    let dll = read_header(&mut Cursor::new(bytes), 256).unwrap();
    assert_eq!(
        assess(&dll, &target("x86_64-pc-windows-msvc")).0,
        InspectionStatus::Mismatch
    );
}

#[test]
fn truncated_headers_and_tampered_metadata_do_not_become_evidence() {
    assert!(read_header(&mut Cursor::new(vec![0; 3]), 3).is_err());
    let mut report = linux_report();
    validate_recorded_metadata(&report).unwrap();
    report.metadata.as_mut().unwrap().glibc_versions = vec!["2.1".into()];
    assert!(validate_recorded_metadata(&report).is_err());
    let mut report = linux_report();
    report.tool.as_mut().unwrap().program = "invented-inspector".into();
    assert!(validate_recorded_metadata(&report).is_err());
    let mut report = linux_report();
    report.tool.as_mut().unwrap().output.clear();
    assert!(validate_recorded_metadata(&report).is_err());
}

#[cfg(unix)]
#[test]
fn metadata_process_failure_output_budget_and_timeout_are_bounded() {
    let path = Path::new(".");
    assert!(run_tool("sh", &["-c", "exit 7"], path).is_err());
    assert!(run_tool("sh", &["-c", "while :; do printf '012345678901234567890123456789012345678901234567890123456789012345'; done"], path).is_err());
    let start = Instant::now();
    assert!(
        run_tool_limited(
            "sh",
            &["-c", "while :; do :; done"],
            path,
            Duration::from_millis(30)
        )
        .is_err()
    );
    assert!(start.elapsed() < Duration::from_secs(2));
}
