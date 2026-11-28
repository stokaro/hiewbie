use super::*;

fn fixture() -> Manifest {
    serde_json::from_str(include_str!("../../../build/targets.json")).unwrap()
}

#[test]
fn complete_manifest_and_explicit_baselines_are_valid() {
    let manifest = fixture();
    manifest.validate().unwrap();
    assert_eq!(
        manifest
            .targets
            .iter()
            .filter(|target| target.required)
            .count(),
        10
    );
    assert_eq!(manifest.supplemental_artifacts.len(), 1);
    for target in &manifest.targets {
        assert_eq!(target.evidence_status, Pending::Pending);
        assert_eq!(target.build.compiler, "1.90.0");
    }
}

#[test]
fn missing_duplicate_or_optional_required_targets_fail() {
    let mut missing = fixture();
    missing.targets.remove(0);
    assert!(
        missing
            .validate()
            .unwrap_err()
            .to_string()
            .contains("missing")
    );
    let mut duplicate = fixture();
    duplicate.targets[1].triple = duplicate.targets[0].triple.clone();
    assert!(duplicate.validate().is_err());
    let mut optional = fixture();
    optional.targets[0].required = false;
    assert!(optional.validate().is_err());
    let mut universal = fixture();
    universal.supplemental_artifacts.clear();
    assert!(universal.validate().is_err());
    let mut universal = fixture();
    universal.supplemental_artifacts[0].inputs.pop();
    assert!(universal.validate().is_err());
}

#[test]
fn archive_and_platform_contracts_cannot_silently_change() {
    let mut manifest = fixture();
    manifest.targets[0].artifact_template = "hiewbie-{version}-mac.dmg".into();
    assert!(manifest.validate().is_err());
    let mut manifest = fixture();
    manifest.targets[0].runtime.system = System::Linux;
    assert!(manifest.validate().is_err());
    let mut manifest = fixture();
    manifest.targets[0].runtime.machine = "x86_64".into();
    assert!(manifest.validate().is_err());
    let mut manifest = fixture();
    manifest.targets[0].runtime.binary_format = BinaryFormat::Elf;
    assert!(manifest.validate().is_err());
    let mut manifest = fixture();
    manifest.supplemental_artifacts[0].artifact_template = "hiewbie-{version}-universal.pkg".into();
    assert!(manifest.validate().is_err());
}

#[test]
fn capability_and_evidence_claims_require_known_contracts() {
    let mut manifest = fixture();
    manifest
        .capability_profiles
        .get_mut("native")
        .unwrap()
        .required
        .push("decode.imaginary".into());
    assert!(
        manifest
            .validate()
            .unwrap_err()
            .to_string()
            .contains("unknown capability")
    );
    let mut manifest = fixture();
    manifest.targets[0]
        .runtime
        .required_evidence
        .push("always-successful".into());
    assert!(manifest.validate().is_err());
    let mut manifest = fixture();
    manifest.targets[0]
        .runtime
        .runtime_checks
        .retain(|check| check != "save");
    assert!(manifest.validate().is_err());
    let mut value = serde_json::to_value(fixture()).unwrap();
    value["targets"][0]["evidence_status"] = "runtime-verified".into();
    assert!(serde_json::from_value::<Manifest>(value).is_err());
}

#[test]
fn build_environment_is_reviewed_and_native_host_is_exact() {
    let mut manifest = fixture();
    manifest.targets[0]
        .build
        .environment
        .insert("LD_PRELOAD".into(), "anything".into());
    assert!(manifest.validate().is_err());
    let mut manifest = fixture();
    manifest.targets[0]
        .build
        .environment
        .insert("MACOSX_DEPLOYMENT_TARGET".into(), "15.0".into());
    assert!(manifest.validate().is_err());
    let mut manifest = fixture();
    if let Ci::Native { host, .. } = &mut manifest.targets[0].ci {
        *host = "different-host".into();
    }
    assert!(manifest.validate().is_err());
}

#[test]
fn version_comparison_uses_numbers_not_lexical_order() {
    assert!(!version_exceeds("2.9", "2.35"));
    assert!(version_exceeds("2.40", "2.35"));
    assert!(!version_exceeds("11.0.0", "11.0"));
    assert!(version_exceeds("private", "2.35"));
}
