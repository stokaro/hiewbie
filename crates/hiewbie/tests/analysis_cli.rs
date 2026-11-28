//! Actual-binary tests use generated integer fields and known instruction bytes.
//! No executable sample or proprietary reference program is redistributed.

use std::{
    ffi::OsStr,
    fs,
    path::Path,
    process::{Command, Output, Stdio},
};

#[path = "../../../tests/fixtures/aarch64_images.rs"]
mod aarch64_images;

fn invoke(path: &Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_hiewbie"))
        .args(arguments)
        .arg(path)
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

fn text(output: &Output) -> String {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!output.stdout.contains(&0x1b));
    String::from_utf8(output.stdout.clone()).unwrap()
}

fn put16(bytes: &mut [u8], at: usize, value: u16) {
    bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
}
fn put32(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}
fn put64(bytes: &mut [u8], at: usize, value: u64) {
    bytes[at..at + 8].copy_from_slice(&value.to_le_bytes());
}

fn pe() -> Vec<u8> {
    let mut bytes = vec![0; 0x400];
    bytes[..2].copy_from_slice(b"MZ");
    put32(&mut bytes, 0x3c, 0x80);
    bytes[0x80..0x84].copy_from_slice(b"PE\0\0");
    put16(&mut bytes, 0x84, 0x14c);
    put16(&mut bytes, 0x86, 1);
    put16(&mut bytes, 0x94, 96);
    put16(&mut bytes, 0x98, 0x10b);
    put32(&mut bytes, 0xa8, 0x1000);
    put32(&mut bytes, 0xb4, 0x40_0000);
    put32(&mut bytes, 0xb8, 0x1000);
    put32(&mut bytes, 0xbc, 0x200);
    put32(&mut bytes, 0xd0, 0x2000);
    put32(&mut bytes, 0xd4, 0x200);
    bytes[0xf8..0xfd].copy_from_slice(b".text");
    put32(&mut bytes, 0x100, 0x100);
    put32(&mut bytes, 0x104, 0x1000);
    put32(&mut bytes, 0x108, 0x200);
    put32(&mut bytes, 0x10c, 0x200);
    put32(&mut bytes, 0x11c, 0x6000_0020);
    bytes[0x200..0x204].copy_from_slice(&[0x90, 0xeb, 0xfd, 0xc3]);
    bytes
}

fn macho(cpu: u32) -> Vec<u8> {
    let mut bytes = vec![0; 0x200];
    put32(&mut bytes, 0, 0xfeed_facf);
    put32(&mut bytes, 4, cpu);
    put32(&mut bytes, 12, 2);
    put32(&mut bytes, 16, 2);
    put32(&mut bytes, 20, 96);
    put32(&mut bytes, 32, 0x19);
    put32(&mut bytes, 36, 72);
    bytes[40..46].copy_from_slice(b"__TEXT");
    put64(&mut bytes, 56, 0x1_0000_0000);
    put64(&mut bytes, 64, 0x1000);
    put64(&mut bytes, 80, 0x200);
    put32(&mut bytes, 88, 7);
    put32(&mut bytes, 92, 5);
    put32(&mut bytes, 104, 0x8000_0028);
    put32(&mut bytes, 108, 24);
    put64(&mut bytes, 112, 0x100);
    if cpu == 0x0100_000c {
        put32(&mut bytes, 0x100, 0xd503_201f); // nop
        put32(&mut bytes, 0x104, 0x17ff_ffff); // b back to the nop
        put32(&mut bytes, 0x108, 0xd65f_03c0); // ret
    } else {
        bytes[0x100..0x104].copy_from_slice(&[0x90, 0xeb, 0xfd, 0xc3]);
    }
    bytes
}

fn fat() -> Vec<u8> {
    let mut bytes = vec![0; 0x600];
    bytes[..4].copy_from_slice(&0xcafe_babe_u32.to_be_bytes());
    bytes[4..8].copy_from_slice(&2_u32.to_be_bytes());
    for (index, cpu) in [0x0100_0007_u32, 0x0100_000c].into_iter().enumerate() {
        let at = 8 + index * 20;
        let start = 0x200 + index * 0x200;
        bytes[at..at + 4].copy_from_slice(&cpu.to_be_bytes());
        bytes[at + 8..at + 12].copy_from_slice(&(start as u32).to_be_bytes());
        bytes[at + 12..at + 16].copy_from_slice(&0x200_u32.to_be_bytes());
        bytes[at + 16..at + 20].copy_from_slice(&9_u32.to_be_bytes());
        bytes[start..start + 0x200].copy_from_slice(&macho(cpu));
    }
    bytes
}

fn many_regions_elf() -> Vec<u8> {
    let mut bytes = vec![0; 0x3000];
    bytes[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
    put16(&mut bytes, 16, 2);
    put16(&mut bytes, 18, 62);
    put32(&mut bytes, 20, 1);
    put64(&mut bytes, 24, 0x40_0000);
    put64(&mut bytes, 32, 64);
    put16(&mut bytes, 52, 64);
    put16(&mut bytes, 54, 56);
    put16(&mut bytes, 56, 100);
    for index in 0..100 {
        let at = 64 + index * 56;
        put32(&mut bytes, at, 1);
        put32(&mut bytes, at + 4, 5);
        put64(&mut bytes, at + 8, (0x2000 + index * 8) as u64);
        put64(&mut bytes, at + 16, (0x40_0000 + index * 8) as u64);
        put64(&mut bytes, at + 32, 1);
        put64(&mut bytes, at + 40, 1);
        put64(&mut bytes, at + 48, 1);
    }
    bytes
}

#[test]
fn pe_headers_and_disassembly_use_file_va_and_declared_x86_profile() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("тест-PE.exe");
    let bytes = pe();
    fs::write(&path, &bytes).unwrap();
    let headers = text(&invoke(&path, &["--headers"]));
    assert!(headers.contains("Format: PE32"));
    assert!(headers.contains("Machine: x86; 32-bit; little-endian"));
    assert!(headers.contains("Optional.AddressOfEntryPoint = 0x1000; FO 00000000000000A8+4"));
    let code = text(&invoke(&path, &["--disassemble", "--entry"]));
    assert!(code.contains("Profile: x86-32; little-endian; backend iced-x86"));
    assert!(code.contains("FO 0000000000000200 VA 0000000000401000  90  nop"));
    assert!(code.contains("-> VA 0000000000401000 (x86-32)"));
    let dump = text(&invoke(&path, &["--dump", "--offset=.401000"]));
    assert!(dump.starts_with("0000000000000200  90 EB FD C3"));
    assert_eq!(fs::read(&path).unwrap(), bytes);
}

#[test]
fn aarch64_macho_is_decoded_independently_of_the_host_cpu() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("arm64-macho");
    fs::write(&path, macho(0x0100_000c)).unwrap();
    let output = text(&invoke(&path, &["--disassemble", "--entry"]));
    assert!(output.contains("Profile: aarch64; little-endian; backend yaxpeax-arm"));
    assert!(output.contains("FO 0000000000000100 VA 0000000100000100  1F2003D5  nop"));
    assert!(output.contains("-> VA 0000000100000100 (aarch64)"));
    assert!(output.contains("Image: Mach-O64; declared AArch64"));
}

#[test]
fn universal_slice_selection_reports_global_file_offsets_and_selected_cpu() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("universal");
    fs::write(&path, fat()).unwrap();
    let first = text(&invoke(&path, &["--disassemble", "--entry"]));
    assert!(first.contains("Selected slice: 0 of 2"));
    assert!(first.contains("Profile: x86-64"));
    let second = text(&invoke(&path, &["--disassemble", "--slice=1", "--entry"]));
    assert!(second.contains("Selected slice: 1 of 2"));
    assert!(second.contains("Profile: aarch64"));
    assert!(second.contains("FO 0000000000000500 VA 0000000100000100"));
    let invalid = invoke(&path, &["--headers", "--slice=2"]);
    assert!(!invalid.status.success());
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("slice 2 is unavailable"));
}

#[test]
fn ordinary_raw_disassembles_and_explicit_architecture_obeys_base_and_byte_order() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("raw.bin");
    fs::write(&path, [0x90, 0xeb, 0xfd, 0x0f]).unwrap();
    let automatic = text(&invoke(&path, &["--disassemble"]));
    assert!(automatic.contains("Profile: x86-32"));
    assert!(automatic.contains("90  nop"));
    assert!(automatic.contains("-> VA 0000000000000000 (x86-32)"));
    assert!(!invoke(&path, &["--disassemble", "--raw"]).status.success());
    let output = text(&invoke(
        &path,
        &["--disassemble", "--raw", "--arch=x86-64", "--base=4096t"],
    ));
    assert!(output.contains("raw; explicit override"));
    assert!(output.contains("Raw base: VA 0000000000001000"));
    assert!(output.contains("-> VA 0000000000001000 (x86-64)"));
    assert!(output.contains("truncated instruction"));
    let unsupported = text(&invoke(
        &path,
        &["--disassemble", "--raw", "--arch=arm", "--endian=big"],
    ));
    assert!(unsupported.contains("unsupported instruction endianness"));
}

#[test]
fn malformed_metadata_fails_clearly_while_raw_dump_and_manual_decode_remain_available() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("malformed.exe");
    let bytes = pe()[..0x100].to_vec();
    fs::write(&path, &bytes).unwrap();
    let headers = invoke(&path, &["--headers"]);
    assert!(!headers.status.success());
    assert!(headers.stdout.is_empty());
    assert!(String::from_utf8_lossy(&headers.stderr).contains("use --dump for raw bytes"));
    assert!(text(&invoke(&path, &["--dump"])).starts_with("0000000000000000  4D 5A"));
    let disassembly = text(&invoke(&path, &["--disassemble", "--arch=x86-32"]));
    assert!(disassembly.contains("Format fallback:"));
    assert_eq!(fs::read(path).unwrap(), bytes);
}

#[test]
fn cli_output_budgets_report_omitted_metadata_and_limit_disassembly_rows() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("many-regions.elf");
    fs::write(&path, many_regions_elf()).unwrap();
    let output = text(&invoke(&path, &["--headers"]));
    assert!(output.contains("36 additional regions omitted"));
    assert!(output.contains("additional fields omitted"));
    assert!(output.lines().count() < 150);
    fs::write(&path, vec![0x90; 4096]).unwrap();
    let code = text(&invoke(&path, &["--disassemble", "--raw", "--arch=x86-64"]));
    assert_eq!(
        code.lines().filter(|line| line.starts_with("FO ")).count(),
        32
    );
}

#[test]
fn conflicting_flags_and_unknown_architectures_fail_before_opening_a_file() {
    for arguments in [
        &["--headers", "--dump"][..],
        &["--disassemble", "--headers"],
        &["--entry", "--offset=0"],
        &["--raw", "--slice=0"],
        &["--base=0", "--slice=0"],
        &["--arch=mips"],
        &["--slice=-1"],
    ] {
        let output = invoke(
            Path::new("not-opened-because-arguments-are-invalid"),
            arguments,
        );
        assert!(!output.status.success());
        assert!(!String::from_utf8_lossy(&output.stderr).contains("cannot open"));
    }
}

#[test]
fn executable_metadata_and_error_messages_do_not_inject_terminal_controls() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("control-name.exe");
    let mut bytes = pe();
    bytes[0xf8..0x100].copy_from_slice(b"\x1b[31m\n\r\0");
    fs::write(&path, bytes).unwrap();
    let headers = text(&invoke(&path, &["--headers"]));
    assert!(headers.contains(".[31m.."));
    let output = Command::new(env!("CARGO_BIN_EXE_hiewbie"))
        .args([OsStr::new("--arch=\x1b[31m"), path.as_os_str()])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!output.stderr.contains(&0x1b));
}

#[test]
fn arm64_big_endian_elf_disassembly_uses_little_endian_code_and_checked_addresses() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("checked-be-arm64.elf");
    fs::write(&path, aarch64_images::arm64_elf(true)).unwrap();

    let disassembly = text(&invoke(&path, &["--disassemble", "--entry"]));
    assert!(
        disassembly.contains("Profile: aarch64; little-endian"),
        "{disassembly}"
    );
    assert!(
        disassembly.contains("Image data byte order: big-endian"),
        "{disassembly}"
    );
    assert!(disassembly.contains("adr x0, 0x140002020"), "{disassembly}");
    assert!(
        disassembly.contains("adrp x1, 0x140002000"),
        "{disassembly}"
    );
    assert!(disassembly.contains("ldr x3, 0x140002030"), "{disassembly}");

    let headers = text(&invoke(&path, &["--headers"]));
    assert!(headers.contains("64-bit; big-endian"), "{headers}");

    let overridden = text(&invoke(
        &path,
        &["--disassemble", "--entry", "--endian", "big"],
    ));
    assert!(
        overridden.contains("unsupported instruction endianness"),
        "{overridden}"
    );
}

#[test]
fn arm64_macho_and_explicit_raw_mode_share_scalable_vector_decoding() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("generated-arm64.macho");
    let mut image = macho(0x0100_000c);
    image[0x100..0x108].copy_from_slice(&[0xe0, 0xe3, 0x98, 0x25, 0x40, 0x38, 0x21, 0x04]);
    fs::write(&path, image).unwrap();
    let mapped = text(&invoke(&path, &["--disassemble", "--entry"]));
    assert!(mapped.contains("E0E39825  ptrue p0.s"), "{mapped}");
    assert!(
        mapped.contains("40382104  eor3 z0.d, z0.d, z1.d, z2.d"),
        "{mapped}"
    );
    let raw = text(&invoke(
        &path,
        &[
            "--disassemble",
            "--raw",
            "--arch",
            "aarch64",
            "--offset",
            "100",
        ],
    ));
    assert!(raw.contains("E0E39825  ptrue p0.s"), "{raw}");
    assert!(
        raw.contains("40382104  eor3 z0.d, z0.d, z1.d, z2.d"),
        "{raw}"
    );
}
