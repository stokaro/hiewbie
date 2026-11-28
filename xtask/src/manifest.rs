use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::Path;

use hiewbie_core::capabilities::{self, Category};
use serde::{Deserialize, Serialize};

use crate::Result;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema_version: u32,
    pub toolchain_source: String,
    pub compiler: String,
    pub msrv: String,
    pub evidence_policy: String,
    pub capability_profiles: BTreeMap<String, CapabilityProfile>,
    pub targets: Vec<Target>,
    pub supplemental_artifacts: Vec<SupplementalArtifact>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityProfile {
    pub required: Vec<String>,
    pub unavailable_on_target: Vec<String>,
    pub note: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub triple: String,
    pub display_name: String,
    pub required: bool,
    pub artifact_template: String,
    pub build: Build,
    pub ci: Ci,
    pub capability_profile: String,
    pub runtime: RuntimeBaseline,
    pub evidence_status: Pending,
    pub runtime_plan: String,
    pub release_blockers: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Build {
    pub package: String,
    pub scope: BuildScope,
    pub rust_std: RustStd,
    pub compiler: String,
    pub msrv: String,
    pub linker: String,
    pub sysroot: String,
    pub native_libraries: Vec<String>,
    pub input_provenance: String,
    pub environment: BTreeMap<String, String>,
    pub prerequisites: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum BuildScope {
    Application,
    PortableCore,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum RustStd {
    Rustup,
    NotDistributed,
}

/// Checked-in plans cannot themselves claim successful artifact verification.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Pending {
    Pending,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum System {
    Macos,
    Windows,
    Linux,
    Freebsd,
    Browser,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum BinaryFormat {
    Elf,
    Macho,
    Pe,
    Wasm,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeBaseline {
    pub id: String,
    pub system: System,
    pub machine: String,
    pub binary_format: BinaryFormat,
    /// Selected minimum versions, not evidence of having run on them.
    pub minimum_versions: BTreeMap<String, String>,
    pub required_features: Vec<String>,
    pub required_evidence: Vec<String>,
    pub runtime_checks: Vec<String>,
    pub selection_note: String,
    pub sources: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "mode", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Ci {
    Native {
        runner: String,
        host: String,
        pty: bool,
    },
    Check {
        runner: String,
        host: String,
    },
    Blocked {
        reason: String,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SupplementalArtifact {
    pub name: String,
    pub required: bool,
    pub artifact_template: String,
    pub inputs: Vec<String>,
    pub required_evidence: Vec<String>,
    pub evidence_status: Pending,
    pub blocker: String,
}

pub fn load(root: &Path) -> Result<Manifest> {
    let path = root.join("build/targets.json");
    let metadata = fs::metadata(&path)?;
    if metadata.len() > 1024 * 1024 {
        return Err("target manifest exceeds the 1 MiB limit".into());
    }
    let bytes = fs::read(&path)?;
    let manifest: Manifest = serde_json::from_slice(&bytes)
        .map_err(|error| format!("invalid {}: {error}", path.display()))?;
    manifest.validate()?;
    let pinned = fs::read_to_string(root.join(&manifest.toolchain_source))?;
    if !pinned
        .lines()
        .any(|line| line.trim() == format!("channel = \"{}\"", manifest.compiler))
    {
        return Err("manifest compiler does not match the pinned toolchain".into());
    }
    Ok(manifest)
}

impl Manifest {
    pub fn target(&self, triple: &str) -> Result<&Target> {
        self.targets
            .iter()
            .find(|target| target.triple == triple)
            .ok_or_else(|| format!("unknown target {triple:?}; use cargo xtask targets").into())
    }

    pub fn profile(&self, target: &Target) -> &CapabilityProfile {
        &self.capability_profiles[&target.capability_profile]
    }

    pub fn validate(&self) -> Result<()> {
        if self.schema_version != 2 || self.targets.len() > 32 {
            return Err("target manifest must use schema version 2 and at most 32 targets".into());
        }
        if self.compiler != "1.90.0"
            || self.msrv != "1.90"
            || self.toolchain_source != "rust-toolchain.toml"
        {
            return Err("compiler/MSRV must match the reviewed Rust 1.90.0 / 1.90 contract".into());
        }
        if self.evidence_policy.trim().is_empty() || self.capability_profiles.is_empty() {
            return Err("manifest needs its evidence policy and capability profiles".into());
        }
        let registry = capabilities::current();
        for (name, profile) in &self.capability_profiles {
            if name.is_empty() || profile.note.is_empty() {
                return Err("capability profiles need names and scope notes".into());
            }
            let mut ids = HashSet::new();
            let mut categories = HashSet::new();
            for id in profile
                .required
                .iter()
                .chain(&profile.unavailable_on_target)
            {
                let capability = registry
                    .get(id)
                    .ok_or_else(|| format!("unknown capability {id:?} in profile {name:?}"))?;
                if !ids.insert(id) {
                    return Err(format!("duplicate capability {id:?} in profile {name:?}").into());
                }
                categories.insert(format!("{:?}", capability.category));
            }
            for category in [
                Category::Decode,
                Category::Assembly,
                Category::References,
                Category::Symbols,
                Category::Extensions,
                Category::Devices,
            ] {
                if !categories.contains(&format!("{category:?}")) {
                    return Err(
                        format!("profile {name:?} omits capability category {category:?}").into(),
                    );
                }
            }
        }

        let contract = required_contract()?;
        let mut triples = HashSet::new();
        let mut artifacts = HashSet::new();
        let mut baselines = HashSet::new();
        for target in &self.targets {
            if target.triple.is_empty() || !triples.insert(target.triple.as_str()) {
                return Err(format!("empty or duplicate target triple {:?}", target.triple).into());
            }
            validate_artifact_name(&target.artifact_template)?;
            if !artifacts.insert(&target.artifact_template) {
                return Err("duplicate artifact name".into());
            }
            if !baselines.insert(&target.runtime.id) {
                return Err("runtime baseline IDs must be unique per target".into());
            }
            if !self
                .capability_profiles
                .contains_key(&target.capability_profile)
            {
                return Err(format!("{} has an unknown capability profile", target.triple).into());
            }
            target.validate(&self.compiler, &self.msrv)?;
            if contract.contains(target.triple.as_str()) && !target.required {
                return Err(format!("{} cannot become optional", target.triple).into());
            }
        }
        if contract.iter().any(|triple| !triples.contains(triple)) {
            return Err("a required target from the build contract is missing".into());
        }
        let mac_inputs: HashSet<_> = self
            .targets
            .iter()
            .filter(|target| target.required && target.runtime.system == System::Macos)
            .map(|target| target.triple.as_str())
            .collect();
        if mac_inputs.len() != 2 {
            return Err("both required macOS architectures must remain present".into());
        }
        let universal: Vec<_> = self
            .supplemental_artifacts
            .iter()
            .filter(|artifact| artifact.name == "macOS universal CLI")
            .collect();
        if universal.len() != 1 || !universal[0].required {
            return Err(
                "the required macOS universal CLI artifact is missing or duplicated".into(),
            );
        }
        for artifact in &self.supplemental_artifacts {
            validate_artifact_name(&artifact.artifact_template)?;
            if artifact.name == "macOS universal CLI"
                && !artifact.artifact_template.ends_with(".tar.gz")
            {
                return Err("universal CLI must remain a portable tar.gz archive".into());
            }
            if !artifacts.insert(&artifact.artifact_template) {
                return Err("duplicate supplemental artifact name".into());
            }
            let inputs: HashSet<_> = artifact.inputs.iter().map(String::as_str).collect();
            if inputs.len() != artifact.inputs.len()
                || inputs.is_empty()
                || inputs.iter().any(|triple| !triples.contains(triple))
            {
                return Err(format!(
                    "invalid inputs for supplemental artifact {:?}",
                    artifact.name
                )
                .into());
            }
            if artifact.name == "macOS universal CLI" && inputs != mac_inputs {
                return Err("universal artifact must retain both macOS inputs".into());
            }
            if artifact.blocker.trim().is_empty() || artifact.required_evidence.is_empty() {
                return Err(
                    "supplemental artifacts need an evidence plan and explicit blocker".into(),
                );
            }
        }
        Ok(())
    }
}

impl Target {
    fn validate(&self, compiler: &str, msrv: &str) -> Result<()> {
        let build = &self.build;
        let (system, format) = if self.triple.contains("-apple-darwin") {
            (System::Macos, BinaryFormat::Macho)
        } else if self.triple.contains("-windows-msvc") {
            (System::Windows, BinaryFormat::Pe)
        } else if self.triple.contains("-linux-gnu") {
            (System::Linux, BinaryFormat::Elf)
        } else if self.triple.contains("-freebsd") {
            (System::Freebsd, BinaryFormat::Elf)
        } else if self.triple == "wasm32-unknown-unknown" {
            (System::Browser, BinaryFormat::Wasm)
        } else {
            return Err("target has no reviewed runtime mapping".into());
        };
        let machine = self.triple.split('-').next().unwrap_or_default();
        let machine = if machine == "riscv64gc" {
            "riscv64"
        } else {
            machine
        };
        if self.runtime.system != system
            || self.runtime.binary_format != format
            || self.runtime.machine != machine
        {
            return Err("runtime OS/machine/format conflicts with the Rust target".into());
        }
        let suffix = if system == System::Windows {
            ".zip"
        } else {
            ".tar.gz"
        };
        if !self.artifact_template.ends_with(suffix) {
            return Err("required target must retain its portable archive kind".into());
        }
        let expected_package = match build.scope {
            BuildScope::Application => "hiewbie",
            BuildScope::PortableCore => "hiewbie-core",
        };
        if build.package != expected_package || build.compiler != compiler || build.msrv != msrv {
            return Err(format!("{} has inconsistent package/compiler/MSRV", self.triple).into());
        }
        if [
            build.linker.as_str(),
            build.sysroot.as_str(),
            build.input_provenance.as_str(),
            build.prerequisites.as_str(),
            self.runtime_plan.as_str(),
        ]
        .iter()
        .any(|value| value.trim().is_empty())
        {
            return Err(format!(
                "{} needs linker, sysroot, input provenance and runtime plan",
                self.triple
            )
            .into());
        }
        for (key, value) in &build.environment {
            if key != "MACOSX_DEPLOYMENT_TARGET"
                || self.runtime.system != System::Macos
                || !valid_version(value)
            {
                return Err(format!("unreviewed build environment setting {key:?}").into());
            }
        }
        if self.runtime.system == System::Macos
            && build.environment.get("MACOSX_DEPLOYMENT_TARGET")
                != self.runtime.minimum_versions.get("os")
        {
            return Err("macOS deployment target must match its selected OS baseline".into());
        }
        let runtime = &self.runtime;
        if runtime.id.is_empty()
            || runtime.machine.is_empty()
            || runtime.selection_note.is_empty()
            || runtime.sources.is_empty()
            || runtime
                .sources
                .iter()
                .any(|url| !url.starts_with("https://"))
        {
            return Err(format!("{} needs a sourced runtime selection", self.triple).into());
        }
        if runtime
            .minimum_versions
            .values()
            .any(|value| !valid_version(value))
        {
            return Err("minimum runtime versions must be numeric dotted versions".into());
        }
        if runtime.system != System::Browser && !runtime.minimum_versions.contains_key("os") {
            return Err("native targets need a selected minimum OS".into());
        }
        if runtime.system == System::Browser {
            if build.scope != BuildScope::PortableCore
                || runtime.binary_format != BinaryFormat::Wasm
                || runtime.required_features.is_empty()
            {
                return Err(
                    "browser target must retain core-only scope and feature requirements".into(),
                );
            }
        } else if build.scope != BuildScope::Application || build.native_libraries.is_empty() {
            return Err(
                "native target needs its application and native library requirements".into(),
            );
        }
        let evidence: HashSet<_> = runtime
            .required_evidence
            .iter()
            .map(String::as_str)
            .collect();
        if evidence.len() != runtime.required_evidence.len()
            || evidence.iter().any(|value| {
                !matches!(
                    *value,
                    "build" | "binary-inspection" | "runtime" | "minimum-runtime"
                )
            })
        {
            return Err("unknown or duplicate required evidence class".into());
        }
        if build
            .native_libraries
            .iter()
            .any(|value| value.trim().is_empty())
        {
            return Err("native library requirements cannot contain empty entries".into());
        }
        if system == System::Linux
            && (!runtime.minimum_versions.contains_key("glibc")
                || !runtime.minimum_versions.contains_key("kernel"))
        {
            return Err("GNU Linux needs selected glibc and kernel baselines".into());
        }
        for required in ["build", "binary-inspection", "runtime", "minimum-runtime"] {
            if !runtime
                .required_evidence
                .iter()
                .any(|value| value == required)
            {
                return Err(format!("{} omits required evidence {required}", self.triple).into());
            }
        }
        let required_checks: &[&str] = if system == System::Browser {
            &[
                "load-offline",
                "import",
                "navigation",
                "edit",
                "export",
                "reimport",
                "keyboard",
                "resize",
            ]
        } else {
            &[
                "startup",
                "version",
                "help",
                "open",
                "navigation",
                "edit",
                "save",
                "reopen",
                "cancel",
                "terminal-cleanup",
            ]
        };
        let checks: HashSet<_> = runtime.runtime_checks.iter().map(String::as_str).collect();
        if checks.len() != runtime.runtime_checks.len()
            || checks.len() != required_checks.len()
            || required_checks.iter().any(|check| !checks.contains(check))
        {
            return Err("runtime scenarios must retain the complete named acceptance suite".into());
        }
        match &self.ci {
            Ci::Native { runner, host, .. } => {
                if runner.is_empty()
                    || host != &self.triple
                    || build.scope != BuildScope::Application
                    || build.rust_std != RustStd::Rustup
                {
                    return Err(format!(
                        "{} native CI requires a matching Rust host and application",
                        self.triple
                    )
                    .into());
                }
            }
            Ci::Check { runner, host } => {
                if runner.is_empty() || host.is_empty() || build.rust_std != RustStd::Rustup {
                    return Err("compile checks require a runner, host and distributed std".into());
                }
            }
            Ci::Blocked { reason } => {
                if reason.trim().is_empty() || self.release_blockers.is_empty() {
                    return Err("blocked targets need explicit reasons".into());
                }
            }
        }
        Ok(())
    }
}

fn validate_artifact_name(name: &str) -> Result<()> {
    if !name.contains("{version}") || name.contains(['/', '\\']) || name.contains("..") {
        return Err("artifact templates must be versioned basenames".into());
    }
    Ok(())
}

fn required_contract() -> Result<HashSet<&'static str>> {
    // The normative specification is a regression contract; all operational
    // target/CI/download selection still comes from the one JSON manifest.
    let table = include_str!("../../docs/build-and-release.md")
        .split("## 3. Required build matrix")
        .nth(1)
        .and_then(|tail| tail.split("\n## ").next())
        .ok_or("required build matrix is absent from the specification")?;
    let names: HashSet<_> = table
        .lines()
        .filter(|line| line.starts_with('|'))
        .filter_map(|line| {
            line.split('|')
                .nth(2)?
                .trim()
                .strip_prefix('`')?
                .strip_suffix('`')
        })
        .collect();
    if names.len() != 10 {
        return Err("the reviewed build contract must retain ten required Rust targets".into());
    }
    Ok(names)
}

pub fn version(value: &str) -> Option<Vec<u32>> {
    if value.is_empty() {
        return None;
    }
    value
        .split('.')
        .map(|part| {
            if part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()) {
                None
            } else {
                part.parse().ok()
            }
        })
        .collect()
}

pub fn valid_version(value: &str) -> bool {
    version(value).is_some()
}

pub fn version_exceeds(actual: &str, selected: &str) -> bool {
    match (version(actual), version(selected)) {
        (Some(mut actual), Some(mut selected)) => {
            let width = actual.len().max(selected.len());
            actual.resize(width, 0);
            selected.resize(width, 0);
            actual > selected
        }
        _ => true,
    }
}

#[cfg(test)]
mod tests;
