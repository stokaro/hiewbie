use super::*;
use crate::inspection::tests::linux_report;

fn fixture() -> (Manifest, Target, ArtifactIdentity, ArtifactEvidence) {
    let manifest: Manifest =
        serde_json::from_str(include_str!("../../../build/targets.json")).unwrap();
    let target = manifest.target("x86_64-unknown-linux-gnu").unwrap().clone();
    let inspection = linux_report();
    let identity = inspection.identity.clone();
    let capabilities = vec!["raw.read".to_owned(), "raw.overwrite".to_owned()];
    let report = ArtifactEvidence {
        schema_version: 1,
        identity: identity.clone(),
        state: EvidenceState::RuntimeVerified,
        build: Some(BuildEvidence {
            identity: identity.clone(),
            compiler: "1.90.0".into(),
            command: vec!["cargo".into(), "build".into(), "--locked".into()],
            inputs: BTreeMap::from([
                ("rustc".into(), "rustc 1.90.0 generated fixture".into()),
                ("linker".into(), "generated linker provenance".into()),
                ("sysroot".into(), "generated sysroot digest".into()),
            ]),
            compiled_capabilities: capabilities.clone(),
            log_ref: "generated://build-fixture".into(),
            passed: true,
        }),
        inspection: Some(inspection),
        runtime: Some(RuntimeEvidence {
            identity: identity.clone(),
            baseline_id: target.runtime.id.clone(),
            environment: RuntimeEnvironment {
                class: RuntimeClass::VirtualMachine,
                system: target.runtime.system,
                machine: target.runtime.machine.clone(),
                versions: target.runtime.minimum_versions.clone(),
                features: target.runtime.required_features.clone(),
                description: "Generated evidence fixture, never an actual runtime claim.".into(),
            },
            checks: target.runtime.runtime_checks.clone(),
            verified_capabilities: capabilities,
            minimum_runtime_tested: true,
            log_ref: "generated://runtime-fixture".into(),
            passed: true,
        }),
    };
    (manifest, target, identity, report)
}

#[test]
fn complete_consistent_producer_evidence_and_json_roundtrip() {
    let (manifest, target, identity, report) = fixture();
    let roundtrip: ArtifactEvidence =
        serde_json::from_slice(&serde_json::to_vec(&report).unwrap()).unwrap();
    validate(&manifest, &target, &identity, &roundtrip).unwrap();
}

#[test]
fn identity_mismatches_are_rejected_at_every_evidence_stage() {
    let (manifest, target, identity, original) = fixture();
    let mut wrong = original.clone();
    wrong.identity.source_commit = "c".repeat(40);
    assert!(validate(&manifest, &target, &identity, &wrong).is_err());
    let mut wrong = original.clone();
    wrong.build.as_mut().unwrap().identity.sha256 = "c".repeat(64);
    assert!(validate(&manifest, &target, &identity, &wrong).is_err());
    let mut wrong = original.clone();
    wrong.inspection.as_mut().unwrap().identity.target = "aarch64-unknown-linux-gnu".into();
    assert!(validate(&manifest, &target, &identity, &wrong).is_err());
    let mut wrong = original.clone();
    wrong.runtime.as_mut().unwrap().identity.source_commit = "d".repeat(40);
    assert!(validate(&manifest, &target, &identity, &wrong).is_err());
    let mut wrong = original;
    wrong.runtime.as_mut().unwrap().identity.size += 1;
    assert!(validate(&manifest, &target, &identity, &wrong).is_err());
}

#[test]
fn unbacked_success_and_unsupported_capabilities_cannot_be_promoted() {
    let (manifest, target, identity, original) = fixture();
    let mut wrong = original.clone();
    wrong.runtime = None;
    assert!(
        validate(&manifest, &target, &identity, &wrong)
            .unwrap_err()
            .to_string()
            .contains("no runtime")
    );
    let mut wrong = original.clone();
    wrong.inspection = None;
    assert!(validate(&manifest, &target, &identity, &wrong).is_err());
    let mut wrong = original.clone();
    wrong
        .build
        .as_mut()
        .unwrap()
        .compiled_capabilities
        .push("assemble.thumb".into());
    assert!(
        validate(&manifest, &target, &identity, &wrong)
            .unwrap_err()
            .to_string()
            .contains("unsupported capability")
    );
    let mut wrong = original.clone();
    wrong
        .runtime
        .as_mut()
        .unwrap()
        .verified_capabilities
        .push("decode.x86-64".into());
    assert!(
        validate(&manifest, &target, &identity, &wrong)
            .unwrap_err()
            .to_string()
            .contains("not recorded as compiled")
    );
    let mut wrong = original;
    wrong
        .runtime
        .as_mut()
        .unwrap()
        .checks
        .retain(|check| check != "terminal-cleanup");
    assert!(validate(&manifest, &target, &identity, &wrong).is_err());
}

#[test]
fn newer_runtime_success_does_not_verify_the_selected_floor() {
    let (manifest, target, identity, mut report) = fixture();
    report
        .runtime
        .as_mut()
        .unwrap()
        .environment
        .versions
        .insert("glibc".into(), "2.40".into());
    assert!(
        validate(&manifest, &target, &identity, &report)
            .unwrap_err()
            .to_string()
            .contains("selected minimum")
    );
    report.runtime.as_mut().unwrap().minimum_runtime_tested = false;
    assert!(validate(&manifest, &target, &identity, &report).is_err());
    report.state = EvidenceState::Inspected;
    validate(&manifest, &target, &identity, &report).unwrap();
}

#[test]
fn recorded_output_and_abi_facts_must_agree() {
    let (manifest, target, identity, original) = fixture();
    let mut wrong = original.clone();
    wrong
        .inspection
        .as_mut()
        .unwrap()
        .metadata
        .as_mut()
        .unwrap()
        .glibc_versions = vec!["2.1".into()];
    assert!(
        validate(&manifest, &target, &identity, &wrong)
            .unwrap_err()
            .to_string()
            .contains("disagree")
    );
    let mut wrong = original.clone();
    wrong
        .inspection
        .as_mut()
        .unwrap()
        .tool
        .as_mut()
        .unwrap()
        .output
        .push_str("changed");
    assert!(validate(&manifest, &target, &identity, &wrong).is_err());
    let mut wrong = original;
    wrong.inspection.as_mut().unwrap().status = InspectionStatus::Unsupported;
    assert!(validate(&manifest, &target, &identity, &wrong).is_err());
}
