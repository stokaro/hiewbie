#!/usr/bin/env python3
# Adapted from OpenAI's gh-fix-ci helper under Apache-2.0.
# Local changes: explicit check outcomes, GitHub.com URL validation, job-first
# diagnostics, and per-invocation request caching. See ../LICENSE.txt.
from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path
from shutil import which
from typing import Any, Iterable, Sequence
from urllib.parse import urlsplit

FAILURE_CONCLUSIONS = {
    "failure",
    "cancelled",
    "timed_out",
    "action_required",
    "startup_failure",
}

FAILURE_STATES = {
    "failure",
    "error",
    "cancelled",
    "timed_out",
    "action_required",
    "startup_failure",
}

FAILURE_BUCKETS = {"fail", "cancel"}
PENDING_STATES = {"pending", "queued", "in_progress", "requested", "waiting", "expected"}
SKIPPED_STATES = {"skipped", "neutral"}

FAILURE_MARKERS = (
    "error",
    "fail",
    "failed",
    "traceback",
    "exception",
    "assert",
    "panic",
    "fatal",
    "timeout",
    "segmentation fault",
)

DEFAULT_MAX_LINES = 80
DEFAULT_CONTEXT_LINES = 12
PENDING_LOG_MARKERS = (
    "still in progress",
    "log will be available when it is complete",
)


class GhResult:
    def __init__(self, returncode: int, stdout: str, stderr: str):
        self.returncode = returncode
        self.stdout = stdout
        self.stderr = stderr


class InspectionCache:
    """Request results shared only by checks in the current invocation."""

    def __init__(self) -> None:
        self.run_metadata: dict[str, dict[str, Any] | None] = {}
        self.run_logs: dict[str, tuple[str, str]] = {}
        self.job_logs: dict[str, tuple[str, str]] = {}
        self.repo_slug: str | None = None
        self.repo_slug_fetched = False


def run_gh_command(args: Sequence[str], cwd: Path) -> GhResult:
    process = subprocess.run(
        ["gh", *args],
        cwd=cwd,
        text=True,
        capture_output=True,
    )
    return GhResult(process.returncode, process.stdout, process.stderr)


def run_gh_command_raw(args: Sequence[str], cwd: Path) -> tuple[int, bytes, str]:
    process = subprocess.run(
        ["gh", *args],
        cwd=cwd,
        capture_output=True,
    )
    stderr = process.stderr.decode(errors="replace")
    return process.returncode, process.stdout, stderr


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Inspect GitHub PR check status and extract bounded failure snippets "
            "from GitHub.com Actions logs."
        ),
        epilog=(
            "Exit codes: 0 = terminal checks without failures (including skipped); "
            "1 = failed checks; 8 = pending checks; "
            "2 = inspection error, unknown status, or no checks."
        ),
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )
    parser.add_argument("--repo", default=".", help="Path inside the target Git repository.")
    parser.add_argument(
        "--pr", default=None, help="PR number or URL (defaults to current branch PR)."
    )
    parser.add_argument("--max-lines", type=int, default=DEFAULT_MAX_LINES)
    parser.add_argument("--context", type=int, default=DEFAULT_CONTEXT_LINES)
    parser.add_argument("--json", action="store_true", help="Emit JSON instead of text output.")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        return inspect_pr(args)
    except OSError as error:
        return report_error(str(error), args.json, args.pr)


def report_error(message: str, as_json: bool, pr_value: str | None) -> int:
    if as_json:
        print(json.dumps({"pr": pr_value, "status": "error", "error": message, "results": []}, indent=2))
    else:
        print(f"Error: {message}", file=sys.stderr)
    return 2


def inspect_pr(args: argparse.Namespace) -> int:
    repo_root = find_git_root(Path(args.repo))
    if repo_root is None:
        return report_error("Not inside a Git repository.", args.json, args.pr)

    if not ensure_gh_available(repo_root):
        return report_error("gh is unavailable or not authenticated; see stderr.", args.json, args.pr)

    pr_value = resolve_pr(args.pr, repo_root)
    if pr_value is None:
        return report_error("Unable to resolve the PR; see stderr.", args.json, args.pr)

    checks = fetch_checks(pr_value, repo_root)
    if checks is None:
        return report_error("Unable to inspect PR checks; see stderr.", args.json, pr_value)

    counts = {"total": len(checks), "passed": 0, "failed": 0, "pending": 0, "skipped": 0, "unknown": 0}
    for check in checks:
        counts[classify_check(check)] += 1
    if not checks:
        status, exit_code = "no_checks", 2
    elif counts["failed"]:
        status, exit_code = "failed", 1
    elif counts["unknown"]:
        status, exit_code = "unknown", 2
    elif counts["pending"]:
        status, exit_code = "pending", 8
    else:
        status, exit_code = "complete", 0

    results = []
    cache = InspectionCache()
    for check in checks:
        if not is_failing(check):
            continue
        results.append(
            analyze_check(
                check,
                repo_root=repo_root,
                max_lines=max(1, args.max_lines),
                context=max(1, args.context),
                cache=cache,
            )
        )

    if args.json:
        print(json.dumps({"pr": pr_value, "status": status, "counts": counts, "results": results}, indent=2))
    else:
        print(f"PR #{pr_value}: {status.replace('_', ' ')}.")
        print("Checks: " + ", ".join(f"{name}={count}" for name, count in counts.items()))
        if results:
            render_results(pr_value, results)

    return exit_code


def find_git_root(start: Path) -> Path | None:
    result = subprocess.run(
        ["git", "rev-parse", "--show-toplevel"],
        cwd=start,
        text=True,
        capture_output=True,
    )
    if result.returncode != 0:
        return None
    return Path(result.stdout.strip())


def ensure_gh_available(repo_root: Path) -> bool:
    if which("gh") is None:
        print("Error: gh is not installed or not on PATH.", file=sys.stderr)
        return False
    result = run_gh_command(["auth", "status"], cwd=repo_root)
    if result.returncode == 0:
        return True
    message = (result.stderr or result.stdout or "").strip()
    print(message or "Error: gh not authenticated.", file=sys.stderr)
    return False


def resolve_pr(pr_value: str | None, repo_root: Path) -> str | None:
    if pr_value:
        return pr_value
    result = run_gh_command(["pr", "view", "--json", "number"], cwd=repo_root)
    if result.returncode != 0:
        message = (result.stderr or result.stdout or "").strip()
        print(message or "Error: unable to resolve PR.", file=sys.stderr)
        return None
    try:
        data = json.loads(result.stdout or "{}")
    except json.JSONDecodeError:
        print("Error: unable to parse PR JSON.", file=sys.stderr)
        return None
    number = data.get("number") if isinstance(data, dict) else None
    if not number:
        print("Error: no PR number found.", file=sys.stderr)
        return None
    return str(number)


def fetch_checks(pr_value: str, repo_root: Path) -> list[dict[str, Any]] | None:
    primary_fields = ["name", "state", "bucket", "link", "startedAt", "completedAt", "workflow"]
    result = run_gh_command(
        ["pr", "checks", pr_value, "--json", ",".join(primary_fields)],
        cwd=repo_root,
    )
    if result.returncode != 0:
        message = "\n".join(filter(None, [result.stderr, result.stdout])).strip()
        available_fields = parse_available_fields(message)
        if available_fields:
            fallback_fields = primary_fields + ["conclusion", "detailsUrl", "status"]
            selected_fields = [field for field in fallback_fields if field in available_fields]
            if not {"state", "bucket", "conclusion", "status"}.intersection(selected_fields):
                print("Error: no usable fields available for gh pr checks.", file=sys.stderr)
                return None
            result = run_gh_command(
                ["pr", "checks", pr_value, "--json", ",".join(selected_fields)],
                cwd=repo_root,
            )
    # gh uses 1 for failed checks and 8 for pending checks while still emitting JSON.
    if result.returncode not in (0, 1, 8):
        message = (result.stderr or result.stdout or "").strip()
        print(message or "Error: gh pr checks failed.", file=sys.stderr)
        return None
    try:
        data = json.loads(result.stdout)
    except json.JSONDecodeError:
        message = result.stderr.strip()
        print(message or "Error: unable to parse checks JSON.", file=sys.stderr)
        return None
    if not isinstance(data, list) or any(not isinstance(check, dict) for check in data):
        print("Error: unexpected checks JSON shape.", file=sys.stderr)
        return None
    if result.returncode != 0 and data and all(
        classify_check(check) in {"passed", "skipped"} for check in data
    ):
        print("Error: gh exit status contradicts terminal check results.", file=sys.stderr)
        return None
    return data


def classify_check(check: dict[str, Any]) -> str:
    conclusion = normalize_field(check.get("conclusion"))
    state = normalize_field(check.get("state") or check.get("status"))
    bucket = normalize_field(check.get("bucket"))
    if conclusion in FAILURE_CONCLUSIONS or state in FAILURE_STATES or bucket in FAILURE_BUCKETS:
        return "failed"
    if state in PENDING_STATES or conclusion in PENDING_STATES or bucket == "pending":
        return "pending"
    if conclusion in SKIPPED_STATES or state in SKIPPED_STATES or bucket == "skipping":
        return "skipped"
    if conclusion == "success" or state == "success" or bucket == "pass":
        return "passed"
    return "unknown"


def is_failing(check: dict[str, Any]) -> bool:
    return classify_check(check) == "failed"


def analyze_check(
    check: dict[str, Any],
    repo_root: Path,
    max_lines: int,
    context: int,
    cache: InspectionCache | None = None,
) -> dict[str, Any]:
    url = check.get("detailsUrl") or check.get("link") or ""
    if not isinstance(url, str):
        url = ""
    run_id = extract_run_id(url)
    job_id = extract_job_id(url)
    base: dict[str, Any] = {
        "name": check.get("name", ""),
        "detailsUrl": url,
        "runId": run_id,
        "jobId": job_id,
    }

    if run_id is None:
        base["status"] = "external"
        base["note"] = "Only github.com Actions run URLs are supported; inspect this check's URL directly."
        return base

    if cache is None:
        cache = InspectionCache()
    if run_id not in cache.run_metadata:
        cache.run_metadata[run_id] = fetch_run_metadata(run_id, repo_root)
    metadata = cache.run_metadata[run_id]
    log_text, log_error, log_status = fetch_check_log(
        run_id=run_id,
        job_id=job_id,
        repo_root=repo_root,
        cache=cache,
    )

    if log_status == "pending":
        base["status"] = "log_pending"
        base["note"] = log_error or "Logs are not available yet."
        if metadata:
            base["run"] = metadata
        return base

    if log_error:
        base["status"] = "log_unavailable"
        base["error"] = log_error
        if metadata:
            base["run"] = metadata
        return base

    snippet = extract_failure_snippet(log_text, max_lines=max_lines, context=context)
    base["status"] = "ok"
    base["run"] = metadata or {}
    base["logSnippet"] = snippet
    return base


def actions_url_match(url: str) -> re.Match[str] | None:
    if not url:
        return None
    try:
        parsed = urlsplit(url)
        if parsed.scheme != "https" or parsed.hostname != "github.com" or parsed.port not in (None, 443):
            return None
        if parsed.username is not None or parsed.password is not None:
            return None
    except ValueError:
        return None
    return re.match(r"^/[^/]+/[^/]+/actions/runs/(\d+)(?:/job/(\d+))?(?:/|$)", parsed.path)


def extract_run_id(url: str) -> str | None:
    match = actions_url_match(url)
    return match.group(1) if match else None


def extract_job_id(url: str) -> str | None:
    match = actions_url_match(url)
    return match.group(2) if match else None


def fetch_run_metadata(run_id: str, repo_root: Path) -> dict[str, Any] | None:
    fields = [
        "conclusion",
        "status",
        "workflowName",
        "name",
        "event",
        "headBranch",
        "headSha",
        "url",
    ]
    result = run_gh_command(["run", "view", run_id, "--json", ",".join(fields)], cwd=repo_root)
    if result.returncode != 0:
        return None
    try:
        data = json.loads(result.stdout or "{}")
    except json.JSONDecodeError:
        return None
    if not isinstance(data, dict):
        return None
    return data


def fetch_check_log(
    run_id: str,
    job_id: str | None,
    repo_root: Path,
    cache: InspectionCache | None = None,
) -> tuple[str, str, str]:
    if cache is None:
        cache = InspectionCache()
    job_error = ""
    if job_id:
        if job_id not in cache.job_logs:
            cache.job_logs[job_id] = fetch_job_log(job_id, repo_root, cache=cache)
        job_log, job_error = cache.job_logs[job_id]
        if not job_error:
            return job_log, "", "ok"

    if run_id not in cache.run_logs:
        cache.run_logs[run_id] = fetch_run_log(run_id, repo_root)
    log_text, log_error = cache.run_logs[run_id]
    if not log_error:
        return log_text, "", "ok"

    error = f"Job log: {job_error}\nRun log: {log_error}" if job_error else log_error
    status = "pending" if is_log_pending_message(log_error) or is_log_pending_message(job_error) else "error"
    return "", error, status


def fetch_run_log(run_id: str, repo_root: Path) -> tuple[str, str]:
    result = run_gh_command(["run", "view", run_id, "--log"], cwd=repo_root)
    if result.returncode != 0:
        error = (result.stderr or result.stdout or "").strip()
        return "", error or "gh run view failed"
    return result.stdout, ""


def fetch_job_log(
    job_id: str,
    repo_root: Path,
    cache: InspectionCache | None = None,
) -> tuple[str, str]:
    if cache is None:
        cache = InspectionCache()
    if not cache.repo_slug_fetched:
        cache.repo_slug = fetch_repo_slug(repo_root)
        cache.repo_slug_fetched = True
    repo_slug = cache.repo_slug
    if not repo_slug:
        return "", "Error: unable to resolve repository name for job logs."
    endpoint = f"/repos/{repo_slug}/actions/jobs/{job_id}/logs"
    returncode, stdout_bytes, stderr = run_gh_command_raw(["api", endpoint], cwd=repo_root)
    if returncode != 0:
        message = (stderr or stdout_bytes.decode(errors="replace")).strip()
        return "", message or "gh api job logs failed"
    if is_zip_payload(stdout_bytes):
        return "", "Job logs returned a zip archive; unable to parse."
    return stdout_bytes.decode(errors="replace"), ""


def fetch_repo_slug(repo_root: Path) -> str | None:
    result = run_gh_command(["repo", "view", "--json", "nameWithOwner"], cwd=repo_root)
    if result.returncode != 0:
        return None
    try:
        data = json.loads(result.stdout or "{}")
    except json.JSONDecodeError:
        return None
    name_with_owner = data.get("nameWithOwner") if isinstance(data, dict) else None
    if not name_with_owner:
        return None
    return str(name_with_owner)


def normalize_field(value: Any) -> str:
    if value is None:
        return ""
    return str(value).strip().lower()


def parse_available_fields(message: str) -> list[str]:
    if "Available fields:" not in message:
        return []
    fields: list[str] = []
    collecting = False
    for line in message.splitlines():
        if "Available fields:" in line:
            collecting = True
            continue
        if not collecting:
            continue
        field = line.strip()
        if not field:
            continue
        fields.append(field)
    return fields


def is_log_pending_message(message: str) -> bool:
    lowered = message.lower()
    return any(marker in lowered for marker in PENDING_LOG_MARKERS)


def is_zip_payload(payload: bytes) -> bool:
    return payload.startswith(b"PK")


def extract_failure_snippet(log_text: str, max_lines: int, context: int) -> str:
    lines = log_text.splitlines()
    if not lines:
        return ""

    marker_index = find_failure_index(lines)
    if marker_index is None:
        return "\n".join(lines[-max_lines:])

    start = max(0, marker_index - context)
    end = min(len(lines), marker_index + context)
    window = lines[start:end]
    if len(window) > max_lines:
        window = window[-max_lines:]
    return "\n".join(window)


def find_failure_index(lines: Sequence[str]) -> int | None:
    for idx in range(len(lines) - 1, -1, -1):
        lowered = lines[idx].lower()
        if any(marker in lowered for marker in FAILURE_MARKERS):
            return idx
    return None


def tail_lines(text: str, max_lines: int) -> str:
    if max_lines <= 0:
        return ""
    lines = text.splitlines()
    return "\n".join(lines[-max_lines:])


def render_results(pr_number: str, results: Iterable[dict[str, Any]]) -> None:
    results_list = list(results)
    print(f"PR #{pr_number}: {len(results_list)} failing checks analyzed.")
    for result in results_list:
        print("-" * 60)
        print(f"Check: {result.get('name', '')}")
        if result.get("detailsUrl"):
            print(f"Details: {result['detailsUrl']}")
        run_id = result.get("runId")
        if run_id:
            print(f"Run ID: {run_id}")
        job_id = result.get("jobId")
        if job_id:
            print(f"Job ID: {job_id}")
        status = result.get("status", "unknown")
        print(f"Status: {status}")

        run_meta = result.get("run", {})
        if run_meta:
            branch = run_meta.get("headBranch", "")
            sha = (run_meta.get("headSha") or "")[:12]
            workflow = run_meta.get("workflowName") or run_meta.get("name") or ""
            conclusion = run_meta.get("conclusion") or run_meta.get("status") or ""
            print(f"Workflow: {workflow} ({conclusion})")
            if branch or sha:
                print(f"Branch/SHA: {branch} {sha}")
            if run_meta.get("url"):
                print(f"Run URL: {run_meta['url']}")

        if result.get("note"):
            print(f"Note: {result['note']}")

        if result.get("error"):
            print(f"Error fetching logs: {result['error']}")
            continue

        snippet = result.get("logSnippet") or ""
        if snippet:
            print("Failure snippet:")
            print(indent_block(snippet, prefix="  "))
        else:
            print("No snippet available.")
    print("-" * 60)


def indent_block(text: str, prefix: str = "  ") -> str:
    return "\n".join(f"{prefix}{line}" for line in text.splitlines())


if __name__ == "__main__":
    raise SystemExit(main())
