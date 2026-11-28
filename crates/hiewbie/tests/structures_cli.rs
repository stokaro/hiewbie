//! Process-level format navigation output and exact native save/reopen evidence.

use std::{
    fs,
    path::Path,
    process::{Command, Output, Stdio},
};

use hiewbie::file::{FileSource, save_document};
use hiewbie_core::{Document, FileOffset, formats};

#[path = "../../../tests/fixtures/macho_records.rs"]
mod macho;
#[path = "../../../tests/fixtures/macho_images.rs"]
#[allow(dead_code)]
mod macho_images;
#[path = "../../../tests/fixtures/te_images.rs"]
mod te;

fn invoke(path: &Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_hiewbie"))
        .args(arguments)
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
fn te_headers_structures_entry_decode_and_native_save_preserve_stripped_header() {
    let directory = tempfile::tempdir().unwrap();
    for (name, mut original, format, architecture) in [
        ("te32.bin", te::te32(), "TE", "x86-32"),
        ("te64.bin", te::te64(), "TE64", "x86-64"),
        ("arm.bin", te::te(0x1c0), "TE", "arm"),
        ("mixed-arm.bin", te::te(0x1c2), "TE", "arm"),
        ("mixed-thumb.bin", te::te_mixed_thumb(), "TE", "thumb"),
        ("thumb.bin", te::te(0x1c4), "TE", "thumb"),
        ("aarch64.bin", te::te(0xaa64), "TE64", "aarch64"),
    ] {
        let path = directory.path().join(name);
        fs::write(&path, &original).unwrap();
        let headers = text(invoke(&path, &["--headers"]));
        assert!(headers.contains(&format!("Format: {format}\n")));
        let output = text(invoke(&path, &["--structures"]));
        assert!(output.contains("Record: FO 0000000000000028+28"));
        assert!(output.contains("Target: FO 0000000000000F08+100"));
        assert!(output.contains("unsupported"));
        assert!(output.contains("debug directory"));
        assert!(output.contains("RSDS"));
        let decode = text(invoke(&path, &["--disassemble", "--entry"]));
        assert!(
            decode.contains(&format!("Profile: {architecture}")),
            "{decode}"
        );
        assert!(decode.contains("0000000000000F18"));
        let mut document = Document::new(FileSource::open(&path).unwrap());
        document.overwrite(FileOffset::new(0xf18), &[0xcc]).unwrap();
        save_document(&mut document).unwrap();
        original[0xf18] = 0xcc;
        assert_eq!(fs::read(&path).unwrap(), original);
        let mut reopened = Document::new(FileSource::open(&path).unwrap());
        let image = formats::inspect(&mut reopened, None).unwrap().unwrap();
        let entry = formats::VirtualAddress::new(image.entry.unwrap().get() & !1);
        assert_eq!(image.va_to_file(entry).unwrap().get(), 0xf18);
        assert!(text(invoke(&path, &["--headers"])).contains("StrippedSize"));
    }
}

#[test]
fn unknown_macho_commands_are_complete_across_pages_and_fat_offsets_are_global() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("commands.o");
    fs::write(&path, macho::unknown_commands(100)).unwrap();
    let first = text(invoke(&path, &["--structures"]));
    assert!(first.contains("Structure records (100); page 0 of 2"));
    assert_eq!(
        first.lines().filter(|line| line.starts_with("  [")).count(),
        64
    );
    assert!(first.contains("Next: --structures --structure-page 1"));
    assert!(first.contains("unsupported"));
    let last = text(invoke(&path, &["--structures", "--structure-page=1"]));
    assert_eq!(
        last.lines().filter(|line| line.starts_with("  [")).count(),
        36
    );
    assert!(last.contains("  [99] load command | unsupported"));
    assert!(last.contains("Record: FO 0000000000000334+8"));
    assert!(!last.contains("Next:"));
    for arguments in [
        vec!["--structures", "--structure-page=2"],
        vec!["--structures", "--structure-page=18446744073709551615"],
        vec!["--headers", "--structure-page=0"],
        vec!["--structures", "--structure-page=-1"],
        vec!["--structures", "--dump"],
    ] {
        assert!(!invoke(&path, &arguments).status.success());
    }
    fs::write(&path, macho::fat_records()).unwrap();
    let fat = text(invoke(&path, &["--structures", "--slice=1"]));
    assert!(fat.contains("Record: FO 000000000000021C+8"));
}

#[test]
fn malformed_te_preserves_raw_cli_and_unknown_machine_does_not_choose_host_decoder() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("firmware.te");
    let mut bytes = te::te(0xffff);
    fs::write(&path, &bytes).unwrap();
    assert!(text(invoke(&path, &["--headers"])).contains("unknown machine width"));
    assert!(
        !invoke(&path, &["--disassemble", "--entry"])
            .status
            .success()
    );
    assert!(
        text(invoke(
            &path,
            &["--disassemble", "--entry", "--arch=x86-32"]
        ))
        .contains("explicit override")
    );
    assert!(!invoke(&path, &["--symbols"]).status.success());
    te::put16(&mut bytes, 6, 39);
    fs::write(&path, &bytes).unwrap();
    assert!(!invoke(&path, &["--structures"]).status.success());
    assert!(!text(invoke(&path, &["--dump"])).is_empty());
    assert_eq!(fs::read(&path).unwrap(), bytes);
}

#[test]
fn macho_thread_entry_and_object_relocation_records_are_host_independent() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("program.macho");
    for (cpu, thumb, architecture) in [
        (7, false, "x86-32"),
        (0x0100_0007, false, "x86-64"),
        (12, false, "arm"),
        (12, true, "thumb"),
        (0x0100_000c, false, "aarch64"),
    ] {
        for big in [false, true] {
            let original = macho_images::unix_thread(cpu, big, thumb);
            fs::write(&path, &original).unwrap();
            assert!(text(invoke(&path, &["--structures"])).contains("LC_UNIXTHREAD"));
            assert!(text(invoke(&path, &["--dump", "--entry"])).contains("0000000000000500"));
            if !big {
                let decode = text(invoke(&path, &["--disassemble", "--entry"]));
                assert!(
                    decode.contains(&format!("Profile: {architecture}")),
                    "{decode}"
                );
            }
            assert_eq!(fs::read(&path).unwrap(), original);
        }
    }
    let mut original = macho_images::object(0x0100_0007, false, &[[4, 0x0400_0002]]);
    fs::write(&path, &original).unwrap();
    let records = text(invoke(&path, &["--structures"]));
    assert!(records.contains("Record: FO 0000000000000600+8"));
    assert!(records.contains("Target: FO 0000000000000404+4"));
    assert!(records.contains("stored=0x12345678"));
    let mut document = Document::new(FileSource::open(&path).unwrap());
    document.overwrite(FileOffset::new(0x600), &[8]).unwrap();
    save_document(&mut document).unwrap();
    original[0x600] = 8;
    assert_eq!(fs::read(&path).unwrap(), original);
    let records = text(invoke(&path, &["--structures"]));
    assert!(records.contains("Target: FO 0000000000000408+4"));
}
