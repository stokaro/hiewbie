mod evidence;
mod inspection;
mod manifest;

use manifest::{BuildScope, Ci, RustStd, Target};
use std::collections::BTreeMap;
use std::env;
use std::error::Error;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const HELP: &str = "hiewbie repository tasks

Usage:
  cargo xtask check
  cargo xtask build --target <rust-triple>
  cargo xtask targets
  cargo xtask inspect --target <triple> --artifact <path> --commit <full-sha>
  cargo xtask validate-evidence --target <triple> --artifact <path> --commit <full-sha> --evidence <json>

check: manifest, format, lint, workspace tests and offline fuzz corpus.
build: locked target build with the selected environment; no runtime claim.
targets: validated target/runtime-intent JSON, including pending evidence.
inspect: hash and inspect metadata without running the artifact; JSON to stdout.
         Unsupported inspection, target mismatch or exceeded baseline fails.
validate-evidence: compare producer records with exact artifact bytes and commit.

Selected runtime floors are intent, not successful target proof. Packaging and
complete release verification remain unimplemented and fail explicitly.
See docs/build-and-release.md for evidence schema, tool limits and producer trust.";

#[derive(Debug, PartialEq, Eq)]
enum Invocation {
    Help,
    Check,
    Build(String),
    Targets,
    Inspect {
        target: String,
        artifact: PathBuf,
        commit: String,
    },
    ValidateEvidence {
        target: String,
        artifact: PathBuf,
        commit: String,
        evidence: PathBuf,
    },
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
    let manifest = manifest::load(&root)?;
    match invocation {
        Invocation::Help => unreachable!("help returned before loading"),
        Invocation::Check => {
            run_cargo(&root, &["fmt", "--all", "--", "--check"], &BTreeMap::new())?;
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
                &BTreeMap::new(),
            )?;
            run_cargo(
                &root,
                &["test", "--workspace", "--locked"],
                &BTreeMap::new(),
            )?;
            run_cargo(
                &root,
                &[
                    "run",
                    "--locked",
                    "--offline",
                    "--package",
                    "hiewbie-fuzz-replay",
                    "--bin",
                    "replay",
                ],
                &BTreeMap::new(),
            )
        }
        Invocation::Build(triple) => {
            let target = manifest.target(&triple)?;
            let args = build_arguments(target)?;
            if target.build.scope == BuildScope::PortableCore {
                eprintln!(
                    "WebAssembly: portable core only; no browser frontend or archive is produced."
                );
            }
            eprintln!("Build prerequisites: {}", target.build.prerequisites);
            eprintln!(
                "Selected baseline: {} (runtime evidence pending)",
                target.runtime.id
            );
            run_cargo(&root, &args, &target.build.environment)
        }
        Invocation::Targets => {
            println!("{}", serde_json::to_string_pretty(&manifest)?);
            Ok(())
        }
        Invocation::Inspect {
            target,
            artifact,
            commit,
        } => {
            let report = inspection::inspect(&artifact, manifest.target(&target)?, &commit)?;
            println!("{}", serde_json::to_string_pretty(&report)?);
            if report.status != inspection::InspectionStatus::Complete
                || report.baseline_verdict == inspection::BaselineVerdict::Exceeds
            {
                return Err("artifact inspection unsupported, mismatched, or exceeds baseline; see JSON diagnostics".into());
            }
            Ok(())
        }
        Invocation::ValidateEvidence {
            target,
            artifact,
            commit,
            evidence: path,
        } => {
            let target = manifest.target(&target)?;
            let identity =
                inspection::ArtifactIdentity::from_file(&artifact, &target.triple, &commit)?;
            let report = evidence::load(&path)?;
            evidence::validate(&manifest, target, &identity, &report)?;
            if let Some(inspection) = &report.inspection {
                inspection::validate_actual_header(&artifact, inspection)?;
            }
            if inspection::ArtifactIdentity::from_file(&artifact, &target.triple, &commit)?
                != identity
            {
                return Err("artifact changed during evidence validation".into());
            }
            println!(
                "{}",
                serde_json::json!({"valid": true, "identity": identity, "state": report.state,
                "scope": "Consistent trusted-producer evidence; not an authenticated attestation or full release gate."})
            );
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
        [command, rest @ ..] if matches!(command.as_str(), "inspect" | "validate-evidence") => {
            let options = parse_options(rest)?;
            let expected = if command == "inspect" { 3 } else { 4 };
            if options.len() != expected {
                return Err("missing or extra inspection/evidence arguments".into());
            }
            let value = |key: &str| {
                options
                    .get(key)
                    .cloned()
                    .ok_or_else(|| format!("missing {key}"))
            };
            let target = value("--target")?;
            let artifact = PathBuf::from(value("--artifact")?);
            let commit = value("--commit")?;
            if !inspection::hex_identifier(&commit, 40) {
                return Err("--commit requires a full lowercase Git SHA".into());
            }
            if command == "inspect" {
                Ok(Invocation::Inspect {
                    target,
                    artifact,
                    commit,
                })
            } else {
                Ok(Invocation::ValidateEvidence {
                    target,
                    artifact,
                    commit,
                    evidence: PathBuf::from(value("--evidence")?),
                })
            }
        }
        [command, ..] if matches!(command.as_str(), "package" | "verify-release") => Err(format!(
            "{command} is not implemented: complete packaging and release verification remain required"
        )),
        _ => Err("invalid arguments; run cargo xtask --help".into()),
    }
}

fn parse_options(args: &[String]) -> std::result::Result<BTreeMap<String, String>, String> {
    if !args.len().is_multiple_of(2) {
        return Err("options require explicit values".into());
    }
    let mut result = BTreeMap::new();
    for pair in args.chunks_exact(2) {
        if !matches!(
            pair[0].as_str(),
            "--target" | "--artifact" | "--commit" | "--evidence"
        ) || pair[1].is_empty()
            || result.insert(pair[0].clone(), pair[1].clone()).is_some()
        {
            return Err("unknown, empty or duplicate option".into());
        }
    }
    Ok(result)
}
fn workspace_root() -> Result<PathBuf> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "xtask must be directly below workspace root".into())
}
fn build_arguments(target: &Target) -> Result<Vec<&str>> {
    if target.build.rust_std == RustStd::NotDistributed {
        let reason = match &target.ci {
            Ci::Blocked { reason } => reason.as_str(),
            _ => "pinned standard library is not distributed",
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
fn run_cargo(root: &Path, args: &[&str], environment: &BTreeMap<String, String>) -> Result<()> {
    let cargo = env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
    eprintln!("> cargo {}", args.join(" "));
    let status = Command::new(&cargo)
        .args(args)
        .envs(environment)
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
    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).into()).collect()
    }
    #[test]
    fn command_arguments_remain_explicit() {
        for values in [
            vec!["build"],
            vec!["build", "--target"],
            vec!["check", "--all-features"],
            vec!["inspect", "--target", "x", "--target", "x"],
        ] {
            assert!(parse_invocation(&args(&values)).is_err());
        }
        let sha = "a".repeat(40);
        assert!(matches!(
            parse_invocation(&args(&[
                "inspect",
                "--artifact",
                "sample",
                "--commit",
                &sha,
                "--target",
                "x"
            ])),
            Ok(Invocation::Inspect { .. })
        ));
        for name in ["package", "verify-release"] {
            assert!(
                parse_invocation(&args(&[name]))
                    .unwrap_err()
                    .contains("not implemented")
            );
        }
    }
    #[test]
    fn explicit_build_scope_and_blocker_are_preserved() {
        let manifest: manifest::Manifest =
            serde_json::from_str(include_str!("../../build/targets.json")).unwrap();
        let wasm = manifest.target("wasm32-unknown-unknown").unwrap();
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
                "--lib"
            ]
        );
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
    #[test]
    fn failed_subprocess_remains_failure() {
        let error = run_cargo(
            &workspace_root().unwrap(),
            &["--hiewbie-test-intentionally-invalid-option"],
            &BTreeMap::new(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("failed with"));
    }
}
