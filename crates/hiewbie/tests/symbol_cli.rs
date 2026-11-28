//! Process-level symbol inspection checks use generated format fields.

use std::{
    fs,
    path::Path,
    process::{Command, Output, Stdio},
};

#[path = "../../../tests/fixtures/symbol_images.rs"]
mod fixtures;

fn invoke(path: &Path, argument: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_hiewbie"))
        .arg(argument)
        .arg(path)
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

fn text(output: Output) -> String {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!output.stdout.contains(&0x1b));
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn cli_distinguishes_definition_import_slot_forwarder_and_source_records() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("symbols.exe");
    let bytes = fixtures::pe64();
    fs::write(&path, &bytes).unwrap();
    let imports = text(invoke(&path, "--imports"));
    assert!(imports.contains("Parsed imports (1)"));
    assert!(imports.contains("Library: demo.dll"));
    assert!(imports.contains(
        "pointer slot VA 0000000140001160 (not resolved external code); file FO 0000000000000360"
    ));
    assert!(imports.contains("Record: FO 0000000000000340+8"));
    assert!(!imports.contains("entry_symbol"));
    let exports = text(invoke(&path, "--exports"));
    assert!(exports.contains("Parsed exports (2)"));
    assert!(exports.contains("export entry_symbol"));
    assert!(exports.contains("VA 0000000140001000; file FO 0000000000000200"));
    assert!(exports.contains("other.forward"));
    assert!(exports.contains("Target: unavailable; no unambiguous file-backed target"));
    let all = text(invoke(&path, "--symbols"));
    assert!(all.contains("Parsed symbols (3)"));
    assert!(all.contains("Coverage notes"));
    assert_eq!(fs::read(path).unwrap(), bytes);
}

#[test]
fn cli_symbol_output_is_bounded_and_escapes_names() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("bounded.exe");
    let mut bytes = fixtures::pe64();
    bytes[0x3a2..0x3aa].copy_from_slice(b"ex\x1b[31m\0");
    fs::write(&path, &bytes).unwrap();
    let output = text(invoke(&path, "--imports"));
    assert!(output.contains("31m"));
    assert!(!output.contains('\x1b'));
    // One hundred ordinal-only exports to the same real code byte.
    fixtures::put32(&mut bytes, 0x414, 100);
    fixtures::put32(&mut bytes, 0x418, 0);
    fixtures::put32(&mut bytes, 0x41c, 0x1300);
    for index in 0..100 {
        fixtures::put32(&mut bytes, 0x500 + index * 4, 0x1000);
    }
    fs::write(&path, &bytes).unwrap();
    let output = text(invoke(&path, "--exports"));
    assert!(output.contains("Parsed exports (100)"));
    assert_eq!(
        output
            .lines()
            .filter(|line| line.starts_with("  ["))
            .count(),
        64
    );
    assert!(output.contains("36 additional exports omitted"));
}

#[test]
fn malformed_symbol_tables_fail_explicitly_without_breaking_other_cli_views() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("malformed.exe");
    let mut bytes = fixtures::pe64();
    fixtures::put32(&mut bytes, 0x300, u32::MAX);
    fs::write(&path, &bytes).unwrap();
    let symbols = invoke(&path, "--symbols");
    assert!(!symbols.status.success());
    assert!(!symbols.stderr.contains(&0x1b));
    assert!(text(invoke(&path, "--headers")).contains("Format: PE32+"));
    assert!(text(invoke(&path, "--dump")).starts_with("0000000000000000"));
    fs::write(&path, b"raw").unwrap();
    let raw = invoke(&path, "--imports");
    assert!(!raw.status.success());
    assert!(String::from_utf8_lossy(&raw.stderr).contains("no executable symbol tables"));
}
