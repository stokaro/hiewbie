#!/usr/bin/env python3
"""Bounded, coverage-guided libFuzzer campaign with reproducible evidence."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import sys
import time
import tomllib


ROOT = Path(__file__).resolve().parents[1]
CONFIG = ROOT / "fuzz/config.json"


def digest_files(directory: Path) -> dict:
    digest = hashlib.sha256()
    files = sorted(path for path in directory.rglob("*") if path.is_file())
    for path in files:
        digest.update(path.relative_to(directory).as_posix().encode() + b"\0")
        digest.update(hashlib.sha256(path.read_bytes()).digest())
    return {"sha256": digest.hexdigest(), "files": len(files)}


def source_digest() -> str:
    paths = subprocess.check_output(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z", "--",
         "fuzz", "crates/hiewbie-core", "tests/fixtures", "Cargo.lock", "Cargo.toml"],
        cwd=ROOT,
    ).split(b"\0")
    digest = hashlib.sha256()
    for name in sorted(set(path for path in paths if path)):
        path = ROOT / os.fsdecode(name)
        if path.is_file():
            digest.update(name + b"\0")
            digest.update(hashlib.sha256(path.read_bytes()).digest())
    return digest.hexdigest()


def execute(command: list[str], log: Path, timeout: float, env: dict) -> int:
    """Kill the complete process group on a deadline, including the fuzzer."""
    with log.open("wb") as output:
        process = subprocess.Popen(command, cwd=ROOT, env=env, stdout=output,
                                   stderr=subprocess.STDOUT, start_new_session=True)
        try:
            return process.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            def signal_group(value: int) -> None:
                try:
                    os.killpg(process.pid, value)
                except ProcessLookupError:
                    pass

            signal_group(signal.SIGTERM)
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                pass
            # The Cargo parent may exit on SIGTERM while a descendant ignores
            # it. Always kill the group, even when the parent already exited.
            signal_group(signal.SIGKILL)
            process.wait(timeout=5)
            output.write(b"\nHarness deadline exceeded; process group terminated.\n")
            return 124


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cargo", default="cargo", help="Cargo executable or environment wrapper")
    parser.add_argument("--output", type=Path, default=ROOT / "fuzz/results")
    parser.add_argument("--target", action="append", help="Repeat to select targets; default: all five")
    parser.add_argument("--seconds", type=int, help="Per-target libFuzzer budget, 1..3600")
    parser.add_argument("--campaign", action="store_true", help="Use the scheduled budget")
    args = parser.parse_args()
    if os.name != "posix":
        parser.error("guided runner requires POSIX process groups; stable replay is portable")
    config = json.loads(CONFIG.read_text())
    targets = args.target or config["targets"]
    if not targets or any(target not in config["targets"] for target in targets) or len(set(targets)) != len(targets):
        parser.error("select distinct target names from fuzz/config.json")
    seconds = args.seconds if args.seconds is not None else config[
        "scheduled_seconds_per_target" if args.campaign else "pull_request_seconds_per_target"
    ]
    if not 1 <= seconds <= 3600:
        parser.error("seconds must be between 1 and 3600")
    nightly = config["nightly"]
    if tomllib.loads((ROOT / "fuzz/libfuzzer/rust-toolchain.toml").read_text())["toolchain"]["channel"] != nightly:
        parser.error("nightly pin disagrees with fuzz/config.json")
    manifest = ROOT / "fuzz/libfuzzer/Cargo.toml"
    if tomllib.loads(manifest.read_text())["dependencies"]["libfuzzer-sys"] != "=" + config["libfuzzer_sys"]:
        parser.error("libfuzzer-sys pin disagrees with fuzz/config.json")
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    report_path = output / "manifest.json"
    if report_path.exists():
        parser.error("output already contains a campaign; choose a fresh directory")
    env = os.environ.copy()
    env["CARGO_NET_OFFLINE"] = "true"
    lock = ROOT / "fuzz/libfuzzer/Cargo.lock"
    original_lock = hashlib.sha256(lock.read_bytes()).hexdigest()
    report = {
        "schema_version": 1,
        "commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
        "working_tree_dirty": bool(subprocess.check_output(
            ["git", "status", "--porcelain", "--untracked-files=all"], cwd=ROOT
        )),
        "source_sha256": source_digest(),
        "config": config,
        "selected_targets": targets,
        "seconds_per_target": seconds,
        "sanitizer": "address",
        "sanitizer_environment": {
            key: env.get(key) for key in ("ASAN_OPTIONS", "LSAN_OPTIONS")
        },
        "overflow_checks": True,
        "libfuzzer_lock_sha256": original_lock,
        "results": [],
    }

    def save() -> None:
        report_path.write_text(json.dumps(report, indent=2) + "\n")

    save()
    cargo = args.cargo
    try:
        version = subprocess.check_output(
            [cargo, "fuzz", "--version"], cwd=ROOT, env=env, text=True, timeout=30
        ).strip()
        report["cargo_fuzz_version"] = version
        if version != "cargo-fuzz " + config["cargo_fuzz"]:
            raise RuntimeError("install the pinned cargo-fuzz version from fuzz/config.json")
        report["cargo_version"] = subprocess.check_output(
            [cargo, "+" + nightly, "--version"], cwd=ROOT, env=env, text=True, timeout=30
        ).strip()
        corpus = output / "corpus"
        prepare = [cargo, "+1.90.0", "run", "--locked", "--offline", "--package",
                   "hiewbie-fuzz-replay", "--bin", "replay", "--", "--write-corpus", str(corpus)]
        if execute(prepare, output / "prepare.log", 300, env):
            raise RuntimeError("stable corpus preparation failed; see prepare.log")
        report["initial_corpus"] = digest_files(corpus)
        check_lock = [cargo, "+" + nightly, "metadata", "--locked", "--offline",
                      "--no-deps", "--format-version", "1", "--manifest-path", str(manifest)]
        if execute(check_lock, output / "metadata.log", 60, env):
            raise RuntimeError("locked fuzz metadata failed; fetch locked dependencies first")
        common = ["--fuzz-dir", str(manifest.parent), "--target-dir", str(output / "build"),
                  "--sanitizer", "address", "--debug-assertions"]
        build = [cargo, "+" + nightly, "fuzz", "build", *common]
        report["build_command"] = build
        if execute(build, output / "build.log", 900, env):
            raise RuntimeError("instrumented fuzz build failed; see build.log")
        if hashlib.sha256(lock.read_bytes()).hexdigest() != original_lock:
            raise RuntimeError("cargo-fuzz changed the checked-in lockfile")
        for target in targets:
            artifacts = output / "artifacts" / target
            artifacts.mkdir(parents=True, exist_ok=True)
            command = [cargo, "+" + nightly, "fuzz", "run", *common, target,
                       str(corpus / target), "--", f"-max_total_time={seconds}",
                       f"-timeout={config['input_timeout_seconds']}",
                       f"-rss_limit_mb={config['rss_limit_mb']}",
                       f"-malloc_limit_mb={config['malloc_limit_mb']}",
                       f"-max_len={config['max_input_bytes']}", f"-seed={config['seed']}",
                       "-print_final_stats=1", f"-artifact_prefix={artifacts}{os.sep}"]
            result = {"target": target, "command": command, "status": "running",
                      "initial_corpus": digest_files(corpus / target)}
            report["results"].append(result)
            save()
            started = time.monotonic()
            status = execute(command, output / f"{target}.log", seconds + 60, env)
            result["elapsed_seconds"] = round(time.monotonic() - started, 3)
            result["exit_code"] = status
            text = (output / f"{target}.log").read_text(errors="replace")
            executions = re.findall(r"stat::number_of_executed_units:\s*(\d+)", text)
            coverage = re.findall(r"\bcov:\s*(\d+)", text)
            result["executions"] = int(executions[-1]) if executions else 0
            result["coverage_edges"] = max(map(int, coverage), default=0)
            observed = result["executions"] > 0 and result["coverage_edges"] > 0
            result["status"] = "passed" if status == 0 and observed else "failed"
            result["final_corpus"] = digest_files(corpus / target)
            save()
            print(f"{target}: {result['status']}; {result['executions']} executions; "
                  f"{result['coverage_edges']} coverage edges", flush=True)
        if hashlib.sha256(lock.read_bytes()).hexdigest() != original_lock:
            raise RuntimeError("campaign changed the checked-in fuzz lockfile")
        report["status"] = "passed" if all(result["status"] == "passed" for result in report["results"]) else "failed"
    except (OSError, RuntimeError, subprocess.CalledProcessError, subprocess.TimeoutExpired) as error:
        report["status"] = "failed"
        report["error"] = str(error)
        print(f"fuzz: {error}", file=sys.stderr)
    finally:
        save()
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
