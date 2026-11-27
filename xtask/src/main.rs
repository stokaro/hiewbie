use std::collections::HashSet;
use std::env;
use std::error::Error;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use serde::{Deserialize, Serialize};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

const HELP: &str = "hiewbie repository tasks

Usage:
  cargo xtask check
  cargo xtask build --target <rust-triple>
  cargo xtask targets

check    Validate the target manifest, formatting, Clippy, and workspace tests.
build    Build the native application, or the portable core for WebAssembly,
         in release mode with locked dependencies. Install the target standard
         library and any cross-linker first; see docs/adr/0003-build-targets.md.
targets  Print build/targets.json, the required target and CI strategy manifest.

Packaging and release verification are not implemented. They fail explicitly.
The target manifest describes requirements and configured checks, not completed
release artifacts or runtime validation.";

#[derive(Debug, PartialEq, Eq)]
enum Invocation {
    Help,
    Check,
    Build(String),
    Targets,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema_version: u32,
    toolchain_source: String,
    evidence_policy: String,
    targets: Vec<Target>,
    supplemental_artifacts: Vec<SupplementalArtifact>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Target {
    triple: String,
    display_name: String,
    required: bool,
    artifact_template: String,
    build: Build,
    ci: Ci,
    runtime_plan: String,
    release_blockers: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Build {
    package: String,
    scope: BuildScope,
    rust_std: RustStd,
    prerequisites: String,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum BuildScope {
    Application,
    PortableCore,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum RustStd {
    Rustup,
    NotDistributed,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "mode", rename_all = "kebab-case", deny_unknown_fields)]
enum Ci {
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

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SupplementalArtifact {
    name: String,
    required: bool,
    artifact_template: String,
    inputs: Vec<String>,
    blocker: String,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("xtask: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let args = env::args_os()
        .skip(1)
        .map(|arg| {
            arg.into_string()
                .map_err(|_| "arguments must be valid UTF-8")
        })
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let invocation = parse_invocation(&args)?;
    if invocation == Invocation::Help {
        println!("{HELP}");
        return Ok(());
    }

    let root = workspace_root()?;
    let manifest = load_manifest(&root)?;
    match invocation {
        Invocation::Help => unreachable!("help returns before loading the manifest"),
        Invocation::Check => {
            run_cargo(&root, &["fmt", "--all", "--", "--check"])?;
            run_cargo(
                &root,
                &[
                    "clippy",
                    "--workspace",
                    "--all-targets",
                    "--locked",
                    "--",
                    "-D",
                    "warnings",
                ],
            )?;
            run_cargo(&root, &["test", "--workspace", "--locked"])
        }
        Invocation::Build(triple) => {
            let target = manifest
                .targets
                .iter()
                .find(|target| target.triple == triple)
                .ok_or_else(|| {
                    format!("unknown target {triple:?}; use `cargo xtask targets` for the manifest")
                })?;
            let args = build_arguments(target)?;
            if target.build.scope == BuildScope::PortableCore {
                eprintln!(
                    "WebAssembly: compiling hiewbie-core only; no browser frontend or web archive is produced."
                );
            }
            eprintln!("Build prerequisites: {}", target.build.prerequisites);
            run_cargo(&root, &args)
        }
        Invocation::Targets => {
            println!("{}", serde_json::to_string_pretty(&manifest)?);
            Ok(())
        }
    }
}

fn parse_invocation(args: &[String]) -> std::result::Result<Invocation, String> {
    match args {
        [] => Ok(Invocation::Help),
        [command] if matches!(command.as_str(), "help" | "--help" | "-h") => Ok(Invocation::Help),
        [command] if command == "check" => Ok(Invocation::Check),
        [command] if command == "targets" => Ok(Invocation::Targets),
        [command, flag, triple] if command == "build" && flag == "--target" => {
            Ok(Invocation::Build(triple.clone()))
        }
        [command, ..] if matches!(command.as_str(), "package" | "verify-release") => Err(format!(
            "{command} is not implemented: packaging, checksums, artifact inspection, and release completeness verification remain required work"
        )),
        _ => Err("invalid arguments; run `cargo xtask --help`".into()),
    }
}

fn workspace_root() -> Result<PathBuf> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "xtask must be located directly below the workspace root".into())
}

fn load_manifest(root: &Path) -> Result<Manifest> {
    let path = root.join("build/targets.json");
    let bytes =
        fs::read(&path).map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let manifest: Manifest = serde_json::from_slice(&bytes)
        .map_err(|error| format!("invalid {}: {error}", path.display()))?;
    manifest.validate()?;
    Ok(manifest)
}

impl Manifest {
    fn validate(&self) -> Result<()> {
        if self.schema_version != 1 || self.targets.is_empty() {
            return Err("target manifest must use schema version 1 and contain targets".into());
        }
        let mut triples = HashSet::new();
        for target in &self.targets {
            if target.triple.is_empty() || !triples.insert(&target.triple) {
                return Err(format!("empty or duplicate target triple {:?}", target.triple).into());
            }
            if target.artifact_template.is_empty() {
                return Err(format!("{} needs an artifact template", target.triple).into());
            }
            let expected_package = match target.build.scope {
                BuildScope::Application => "hiewbie",
                BuildScope::PortableCore => "hiewbie-core",
            };
            if target.build.package != expected_package {
                return Err(format!(
                    "{} has package {:?}, expected {expected_package} for its build scope",
                    target.triple, target.build.package
                )
                .into());
            }
            if target.triple.starts_with("wasm") && target.build.scope != BuildScope::PortableCore {
                return Err("WebAssembly currently supports portable-core compilation only".into());
            }
            match &target.ci {
                Ci::Native { runner, host, .. } => {
                    if runner.is_empty()
                        || host != &target.triple
                        || target.build.scope != BuildScope::Application
                        || target.build.rust_std != RustStd::Rustup
                    {
                        return Err(format!(
                            "{} native CI requires a matching Rust host, runner, application, and distributed standard library",
                            target.triple
                        )
                        .into());
                    }
                }
                Ci::Check { runner, host } => {
                    if runner.is_empty()
                        || host.is_empty()
                        || target.build.rust_std != RustStd::Rustup
                    {
                        return Err(format!(
                            "{} compile checks need a runner, host, and distributed standard library",
                            target.triple
                        )
                        .into());
                    }
                }
                Ci::Blocked { reason } => {
                    if reason.trim().is_empty() || target.release_blockers.is_empty() {
                        return Err(format!("{} needs an explicit blocker", target.triple).into());
                    }
                }
            }
        }
        for artifact in &self.supplemental_artifacts {
            if artifact.inputs.is_empty()
                || artifact
                    .inputs
                    .iter()
                    .any(|triple| !triples.contains(triple))
            {
                return Err(format!(
                    "supplemental artifact {:?} must name existing input targets",
                    artifact.name
                )
                .into());
            }
        }
        Ok(())
    }
}

fn build_arguments(target: &Target) -> Result<Vec<&str>> {
    if target.build.rust_std == RustStd::NotDistributed {
        let reason = match &target.ci {
            Ci::Blocked { reason } => reason.as_str(),
            _ => "the pinned toolchain does not distribute this target's standard library",
        };
        return Err(format!("{} build is blocked: {reason}", target.triple).into());
    }
    let mut args = vec![
        "build",
        "--release",
        "--locked",
        "--package",
        &target.build.package,
        "--target",
        &target.triple,
    ];
    match target.build.scope {
        BuildScope::Application => args.extend(["--bin", "hiewbie"]),
        BuildScope::PortableCore => args.push("--lib"),
    }
    Ok(args)
}

fn run_cargo(root: &Path, args: &[&str]) -> Result<()> {
    let cargo = env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
    eprintln!("> cargo {}", args.join(" "));
    let status = Command::new(&cargo)
        .args(args)
        .current_dir(root)
        .status()
        .map_err(|error| format!("cannot start {}: {error}", Path::new(&cargo).display()))?;
    if !status.success() {
        return Err(format!("cargo {} failed with {status}", args.join(" ")).into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> Manifest {
        serde_json::from_str(include_str!("../../build/targets.json")).unwrap()
    }

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).into()).collect()
    }

    #[test]
    fn build_requires_an_explicit_target_and_rejects_extra_arguments() {
        assert!(parse_invocation(&args(&["build"])).is_err());
        assert!(parse_invocation(&args(&["build", "--target"])).is_err());
        assert!(parse_invocation(&args(&["check", "--all-features"])).is_err());
        assert_eq!(
            parse_invocation(&args(&["build", "--target", "example-target"])),
            Ok(Invocation::Build("example-target".into()))
        );
    }

    #[test]
    fn incomplete_release_commands_fail() {
        for command in ["package", "verify-release"] {
            let error = parse_invocation(&args(&[command])).unwrap_err();
            assert!(error.contains("not implemented"));
        }
    }

    #[test]
    fn repository_manifest_is_valid() {
        manifest().validate().unwrap();
    }

    #[test]
    fn manifest_retains_the_complete_documented_target_contract() {
        let contract = include_str!("../../docs/build-and-release.md");
        let matrix = contract
            .split("## 3. Required build matrix")
            .nth(1)
            .expect("the build contract must retain its required target matrix")
            .split("\n## ")
            .next()
            .unwrap();
        let documented: HashSet<_> = matrix
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
        assert!(
            !documented.is_empty(),
            "the required target table is missing"
        );
        let manifest = manifest();
        let configured: HashSet<_> = manifest
            .targets
            .iter()
            .filter(|target| target.required)
            .map(|target| target.triple.as_str())
            .collect();
        assert_eq!(configured, documented);
    }

    #[test]
    fn failed_cargo_subprocess_is_not_reported_as_success() {
        let error = run_cargo(
            &workspace_root().unwrap(),
            &["--hiewbie-test-intentionally-invalid-option"],
        )
        .unwrap_err();
        assert!(error.to_string().contains("failed with"));
    }

    #[test]
    fn native_ci_cannot_claim_a_cross_target_as_native() {
        let mut manifest = manifest();
        let native = manifest
            .targets
            .iter_mut()
            .find(|target| matches!(target.ci, Ci::Native { .. }))
            .unwrap();
        if let Ci::Native { host, .. } = &mut native.ci {
            *host = "incorrect-host".into();
        }
        assert!(
            manifest
                .validate()
                .unwrap_err()
                .to_string()
                .contains("matching Rust host")
        );
    }

    #[test]
    fn duplicate_targets_are_rejected() {
        let mut manifest = manifest();
        manifest.targets[1].triple = manifest.targets[0].triple.clone();
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn wasm_build_is_locked_release_core_only() {
        let manifest = manifest();
        let wasm = manifest
            .targets
            .iter()
            .find(|target| target.triple == "wasm32-unknown-unknown")
            .unwrap();
        assert_eq!(
            build_arguments(wasm).unwrap(),
            [
                "build",
                "--release",
                "--locked",
                "--package",
                "hiewbie-core",
                "--target",
                "wasm32-unknown-unknown",
                "--lib",
            ]
        );
    }

    #[test]
    fn unavailable_std_blocks_build_instead_of_substituting_the_host() {
        let manifest = manifest();
        let blocked = manifest
            .targets
            .iter()
            .find(|target| target.build.rust_std == RustStd::NotDistributed)
            .unwrap();
        assert!(
            build_arguments(blocked)
                .unwrap_err()
                .to_string()
                .contains("blocked")
        );
    }
}
