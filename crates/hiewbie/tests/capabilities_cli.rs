use std::process::{Command, Output, Stdio};

fn invoke(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_hiewbie"))
        .args(arguments)
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

#[test]
fn capability_json_equals_the_shared_core_report_without_opening_a_file() {
    let output = invoke(&["--capabilities", "--json"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    assert!(!output.stdout.contains(&0x1B));
    let actual: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        actual,
        serde_json::to_value(hiewbie_core::capabilities::current()).unwrap()
    );
    assert_eq!(actual["schema_version"], 1);
    let rows = actual["capabilities"].as_array().unwrap();
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("../../../tests/fixtures/capabilities.json")).unwrap();
    for case in cases["known"].as_array().unwrap() {
        let entry = rows.iter().find(|entry| entry["id"] == case["id"]).unwrap();
        assert_eq!(entry["status"], case["status"], "{}", case["id"]);
        assert_eq!(entry["category"], case["category"], "{}", case["id"]);
    }
    for absent in cases["absent"].as_array().unwrap() {
        assert!(!rows.iter().any(|entry| entry["id"] == *absent));
    }
}

#[test]
fn text_report_distinguishes_research_and_independent_operations() {
    let output = invoke(&["--capabilities"]);
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("target runtime evidence is separate"));
    assert!(text.contains("iced-x86 1.22.0"));
    for id in [
        "decode.x86-64",
        "assemble.x86-64",
        "symbols.pe",
        "format.pe",
        "references.x86",
        "extensions.portable",
        "devices.raw",
    ] {
        assert!(text.contains(id), "{id}");
    }
    assert!(
        text.lines()
            .find(|line| line.starts_with("assemble.aarch64 "))
            .unwrap()
            .contains("limited")
    );
    assert!(!text.contains('\x1B'));
}

#[test]
fn report_rejects_files_conflicting_options_and_unscoped_json() {
    for args in [
        &["--json"][..],
        &["--capabilities", "missing.bin"],
        &["--capabilities", "--dump"],
        &["--capabilities", "--arch=auto"],
        &["--capabilities", "--raw"],
        &["--capabilities", "--read-only"],
        &["--capabilities", "--capabilities"],
        &["--capabilities", "--json", "--json"],
    ] {
        let output = invoke(args);
        assert!(!output.status.success(), "accepted {args:?}");
        assert!(output.stdout.is_empty());
        assert!(!output.stderr.contains(&0x1B));
    }
    let output = invoke(&["--json", "--capabilities"]);
    assert!(output.status.success());
    assert!(serde_json::from_slice::<serde_json::Value>(&output.stdout).is_ok());
}
