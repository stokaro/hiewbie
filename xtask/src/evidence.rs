//! Validate producer evidence without treating descriptions as execution proof.
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::Path;

use hiewbie_core::capabilities;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::Result;
use crate::inspection::{
    ArtifactIdentity, BaselineVerdict, InspectionReport, InspectionStatus, assess,
};
use crate::manifest::{Manifest, System, Target, version};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum EvidenceState {
    Proposed,
    Built,
    Inspected,
    RuntimeVerified,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactEvidence {
    pub schema_version: u32,
    pub identity: ArtifactIdentity,
    pub state: EvidenceState,
    pub build: Option<BuildEvidence>,
    pub inspection: Option<InspectionReport>,
    pub runtime: Option<RuntimeEvidence>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BuildEvidence {
    pub identity: ArtifactIdentity,
    pub compiler: String,
    pub command: Vec<String>,
    pub inputs: BTreeMap<String, String>,
    pub compiled_capabilities: Vec<String>,
    pub log_ref: String,
    pub passed: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeClass {
    Native,
    VirtualMachine,
    Emulated,
    Browser,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeEnvironment {
    pub class: RuntimeClass,
    pub system: System,
    pub machine: String,
    pub versions: BTreeMap<String, String>,
    pub features: Vec<String>,
    pub description: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeEvidence {
    pub identity: ArtifactIdentity,
    pub baseline_id: String,
    pub environment: RuntimeEnvironment,
    pub checks: Vec<String>,
    pub verified_capabilities: Vec<String>,
    pub minimum_runtime_tested: bool,
    pub log_ref: String,
    pub passed: bool,
}

pub fn load(path: &Path) -> Result<ArtifactEvidence> {
    if fs::metadata(path)?.len() > 4 * 1024 * 1024 {
        return Err("evidence exceeds the 4 MiB limit".into());
    }
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

pub fn validate(
    manifest: &Manifest,
    target: &Target,
    expected: &ArtifactIdentity,
    report: &ArtifactEvidence,
) -> Result<()> {
    expected.validate()?;
    if report.schema_version != 1 || &report.identity != expected {
        return Err(
            "evidence does not match the requested target, source commit, artifact digest and size"
                .into(),
        );
    }
    if expected.target != target.triple {
        return Err("evidence target does not match selected manifest target".into());
    }
    if let Some(build) = &report.build {
        if &build.identity != expected || !build.passed || build.compiler != target.build.compiler {
            return Err(
                "build evidence has mismatched identity/compiler or an unsuccessful result".into(),
            );
        }
        if build.command.is_empty()
            || build.log_ref.trim().is_empty()
            || build.command.iter().any(|value| value.trim().is_empty())
        {
            return Err("build evidence needs the executed command and its log reference".into());
        }
        for input in ["rustc", "linker", "sysroot"] {
            if build
                .inputs
                .get(input)
                .is_none_or(|value| value.trim().is_empty())
            {
                return Err(format!("build evidence is missing {input} provenance").into());
            }
        }
        validate_capabilities(manifest, target, &build.compiled_capabilities)?;
    }

    if let Some(inspection) = &report.inspection {
        if inspection.schema_version != 1
            || &inspection.identity != expected
            || inspection.baseline_id != target.runtime.id
            || inspection.status != InspectionStatus::Complete
            || inspection.baseline_verdict == BaselineVerdict::Exceeds
        {
            return Err("inspection evidence mismatches the artifact/baseline, is unsupported, or exceeds the baseline".into());
        }
        let metadata = inspection
            .metadata
            .as_ref()
            .ok_or("inspection lacks binary metadata")?;
        let tool = inspection
            .tool
            .as_ref()
            .ok_or("inspection lacks metadata-tool evidence")?;
        if tool.output.is_empty()
            || tool.arguments.is_empty()
            || tool.output_sha256 != format!("{:x}", Sha256::digest(tool.output.as_bytes()))
        {
            return Err("inspection tool output or its digest is missing/inconsistent".into());
        }
        crate::inspection::validate_recorded_metadata(inspection)?;
        let (status, verdict, _) = assess(metadata, target);
        if status != inspection.status || verdict != inspection.baseline_verdict {
            return Err("inspection claim disagrees with the recorded binary metadata".into());
        }
    }

    if let Some(runtime) = &report.runtime {
        if &runtime.identity != expected
            || runtime.baseline_id != target.runtime.id
            || !runtime.passed
        {
            return Err(
                "runtime evidence has mismatched identity/baseline or an unsuccessful result"
                    .into(),
            );
        }
        let environment = &runtime.environment;
        if environment.system != target.runtime.system
            || environment.machine != target.runtime.machine
            || environment.description.trim().is_empty()
            || runtime.log_ref.trim().is_empty()
        {
            return Err(
                "runtime evidence needs the matching OS/CPU, environment and log reference".into(),
            );
        }
        if (environment.class == RuntimeClass::Browser) != (environment.system == System::Browser) {
            return Err("browser runtime evidence requires an actual browser environment".into());
        }
        if environment
            .versions
            .values()
            .any(|value| version(value).is_none())
        {
            return Err("runtime versions must be explicit numeric dotted versions".into());
        }
        let checks: HashSet<_> = runtime.checks.iter().collect();
        if checks.len() != runtime.checks.len()
            || target
                .runtime
                .runtime_checks
                .iter()
                .any(|check| !checks.contains(check))
        {
            return Err("runtime evidence omits required scenarios or duplicates a check".into());
        }
        validate_capabilities(manifest, target, &runtime.verified_capabilities)?;
        let build = report
            .build
            .as_ref()
            .ok_or("runtime claims need matching successful build evidence")?;
        if runtime
            .verified_capabilities
            .iter()
            .any(|id| !build.compiled_capabilities.contains(id))
        {
            return Err(
                "runtime-verified capability was not recorded as compiled for this artifact".into(),
            );
        }
        if runtime.minimum_runtime_tested {
            for (component, selected) in &target.runtime.minimum_versions {
                let actual = environment
                    .versions
                    .get(component)
                    .ok_or_else(|| format!("minimum-runtime evidence lacks {component}"))?;
                if !same_selected_version(actual, selected) {
                    return Err(format!("runtime {component}={actual} does not test selected minimum {selected}; newer runtime success is separate evidence").into());
                }
            }
            for feature in &target.runtime.required_features {
                if !environment.features.contains(feature) {
                    return Err(format!(
                        "minimum-runtime evidence lacks required feature {feature:?}"
                    )
                    .into());
                }
            }
            if environment.system == System::Browser
                && !environment.versions.contains_key("browser")
            {
                return Err("browser evidence must identify the tested browser version".into());
            }
        }
    }

    match report.state {
        EvidenceState::Proposed => {}
        EvidenceState::Built => {
            report
                .build
                .as_ref()
                .ok_or("built claim has no build result")?;
        }
        EvidenceState::Inspected => {
            report
                .build
                .as_ref()
                .ok_or("inspected claim has no build result")?;
            report
                .inspection
                .as_ref()
                .ok_or("inspected claim has no inspection result")?;
        }
        EvidenceState::RuntimeVerified => {
            report
                .build
                .as_ref()
                .ok_or("runtime-verified claim has no build result")?;
            report
                .inspection
                .as_ref()
                .ok_or("runtime-verified claim has no inspection result")?;
            let runtime = report
                .runtime
                .as_ref()
                .ok_or("runtime-verified claim has no runtime result")?;
            if !runtime.minimum_runtime_tested {
                return Err("runtime-verified baseline requires a minimum-runtime result, not only a newer-runner test".into());
            }
        }
    }
    Ok(())
}

fn validate_capabilities(manifest: &Manifest, target: &Target, ids: &[String]) -> Result<()> {
    let registry = capabilities::current();
    let profile = manifest.profile(target);
    let mut unique = HashSet::new();
    for id in ids {
        if !unique.insert(id) {
            return Err(format!("duplicate capability evidence {id:?}").into());
        }
        if registry.require(id).is_err() || profile.unavailable_on_target.contains(id) {
            return Err(format!(
                "unsupported capability cannot be claimed compiled/verified: {id}"
            )
            .into());
        }
    }
    Ok(())
}

fn same_selected_version(actual: &str, selected: &str) -> bool {
    match (version(actual), version(selected)) {
        (Some(actual), Some(selected)) => actual.starts_with(&selected),
        _ => false,
    }
}

#[cfg(test)]
mod tests;
