use std::{
    fs,
    process::{Command, Output, Stdio},
};

fn invoke(arguments: &[&std::ffi::OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_hiewbie"))
        .args(arguments)
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

#[test]
fn dump_round_trip_uses_unicode_path_and_requested_offset() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("тест-試験.bin");
    fs::write(&path, b"\x00\x1BHello\xFF").unwrap();
    let result = invoke(&[
        "--dump".as_ref(),
        "--offset".as_ref(),
        "2t".as_ref(),
        path.as_os_str(),
    ]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        String::from_utf8(result.stdout).unwrap(),
        "0000000000000002  48 65 6C 6C 6F FF                                 |Hello.|\n"
    );
}

#[test]
fn interactive_mode_refuses_nonterminal_streams_without_escape_sequences() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("file.bin");
    fs::write(&path, b"original").unwrap();
    let result = invoke(&[path.as_os_str()]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("use --dump"));
    assert!(!result.stdout.contains(&0x1B));
    assert!(!result.stderr.contains(&0x1B));
    assert_eq!(fs::read(path).unwrap(), b"original");
}

#[test]
fn invalid_offset_and_missing_file_return_failure() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("file.bin");
    fs::write(&path, b"original").unwrap();
    let result = invoke(&[
        "--dump".as_ref(),
        "--offset=FFFFFFFFFFFFFFFF".as_ref(),
        path.as_os_str(),
    ]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("invalid --offset"));
    let missing = directory.path().join("missing");
    let result = invoke(&["--dump".as_ref(), missing.as_os_str()]);
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("cannot open"));
}

#[cfg(unix)]
#[test]
fn dump_opens_non_utf8_filename_and_escapes_error_paths() {
    use std::os::unix::ffi::OsStrExt;
    let directory = tempfile::tempdir().unwrap();
    let path = directory
        .path()
        .join(std::ffi::OsStr::from_bytes(b"file-\xFF\x1B.bin"));
    fs::write(&path, b"A").unwrap();
    assert!(
        invoke(&["--dump".as_ref(), path.as_os_str()])
            .status
            .success()
    );
    fs::remove_file(&path).unwrap();
    let result = invoke(&["--dump".as_ref(), path.as_os_str()]);
    assert!(!result.status.success());
    assert!(!result.stderr.contains(&0x1B));
    assert!(String::from_utf8_lossy(&result.stderr).contains("\\u{1b}"));
}
