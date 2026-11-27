#!/usr/bin/env python3
"""Offline regression tests for the locally adapted OpenAI CI helper."""

from __future__ import annotations

import argparse
import contextlib
import io
import json
import unittest
from pathlib import Path
from unittest.mock import patch

import inspect_pr_checks as helper


class InspectPrChecksTests(unittest.TestCase):
    def setUp(self) -> None:
        # Unexpected subprocesses fail instead of accessing git, authentication,
        # or the network. All gh responses below are local fixtures.
        guard = patch.object(helper.subprocess, "run", side_effect=AssertionError("Unexpected subprocess"))
        guard.start()
        self.addCleanup(guard.stop)
        self.run_log = helper.GhResult(0, "setup\nERROR: run failure\ncleanup\n", "")
        self.job_logs: dict[str, tuple[int, bytes, str]] = {}

    def failed_check(self, name: str = "test", job_id: str | None = "456") -> dict:
        link = "https://github.com/example/project/actions/runs/123"
        if job_id:
            link += f"/job/{job_id}"
        return {"name": name, "state": "FAILURE", "bucket": "fail", "link": link}

    def invoke(self, checks, gh_exit=0, *, responses=None, as_json=True):
        self.calls, self.raw_calls = [], []
        if responses is None:
            stdout = checks if isinstance(checks, str) else json.dumps(checks)
            responses = [helper.GhResult(gh_exit, stdout, "")]
        remaining = list(responses)

        def gh(args, cwd):
            self.calls.append(list(args))
            if args[:2] == ["pr", "checks"]:
                return remaining.pop(0)
            if args[:2] == ["run", "view"] and "--json" in args:
                return helper.GhResult(0, '{"conclusion": "failure", "workflowName": "CI"}', "")
            if args[:2] == ["run", "view"] and "--log" in args:
                return self.run_log
            if args == ["repo", "view", "--json", "nameWithOwner"]:
                return helper.GhResult(0, '{"nameWithOwner": "example/project"}', "")
            raise AssertionError(f"Unexpected gh request: {args}")

        def gh_raw(args, cwd):
            self.raw_calls.append(list(args))
            self.assertEqual(args[0], "api")
            job_id = args[1].split("/")[-2]
            self.assertEqual(args[1], f"/repos/example/project/actions/jobs/{job_id}/logs")
            default_log = (0, f"setup\nERROR: job {job_id}\ncleanup\n".encode(), "")
            return self.job_logs.get(job_id, default_log)

        args = argparse.Namespace(repo=".", pr="7", max_lines=80, context=12, json=as_json)
        stdout, stderr = io.StringIO(), io.StringIO()
        with (
            patch.object(helper, "parse_args", return_value=args),
            patch.object(helper, "find_git_root", return_value=Path("/offline-repo")),
            patch.object(helper, "ensure_gh_available", return_value=True),
            patch.object(helper, "run_gh_command", side_effect=gh),
            patch.object(helper, "run_gh_command_raw", side_effect=gh_raw),
            contextlib.redirect_stdout(stdout),
            contextlib.redirect_stderr(stderr),
        ):
            exit_code = helper.main()
        self.assertEqual(remaining, [], "Not all check responses were used")
        self.stderr = stderr.getvalue()
        # Parse the complete output, rejecting any additional prose in JSON mode.
        output = json.loads(stdout.getvalue()) if as_json else stdout.getvalue()
        return exit_code, output

    def test_failed_json_exit_one_is_actionable_and_uses_current_fields(self):
        exit_code, output = self.invoke([self.failed_check()], gh_exit=1)
        self.assertEqual((exit_code, output["status"]), (1, "failed"))
        self.assertEqual(output["counts"]["failed"], 1)
        result = output["results"][0]
        self.assertIn("ERROR: job 456", result["logSnippet"])
        self.assertNotIn("logTail", result)
        calls = [call for call in self.calls if call[:2] == ["pr", "checks"]]
        self.assertEqual(len(calls), 1)
        self.assertEqual(calls[0][-1], "name,state,bucket,link,startedAt,completedAt,workflow")

    def test_pending_exit_eight_stays_json_without_log_requests(self):
        exit_code, output = self.invoke(
            [{"name": "build", "state": "IN_PROGRESS", "bucket": "pending"}], gh_exit=8
        )
        self.assertEqual((exit_code, output["status"]), (8, "pending"))
        self.assertEqual(output["counts"]["pending"], 1)
        self.assertEqual(output["results"], [])
        self.assertEqual(len(self.calls), 1)
        self.assertEqual(self.raw_calls, [])

    def test_terminal_success_and_skipped_checks_are_counted(self):
        exit_code, output = self.invoke([
            {"name": "test", "state": "SUCCESS", "bucket": "pass"},
            {"name": "optional", "state": "SKIPPED", "bucket": "skipping"},
            {"name": "informational", "state": "NEUTRAL"},
        ])
        self.assertEqual((exit_code, output["status"]), (0, "complete"))
        self.assertEqual((output["counts"]["passed"], output["counts"]["skipped"]), (1, 2))

    def test_all_skipped_text_output_makes_skips_visible(self):
        exit_code, output = self.invoke([{"name": "optional", "bucket": "skipping"}], as_json=False)
        self.assertEqual(exit_code, 0)
        self.assertIn("passed=0", output)
        self.assertIn("skipped=1", output)

    def test_absent_or_unknown_checks_never_report_success(self):
        for checks, status in [
            ([], "no_checks"),
            ([{}], "unknown"),
            ([{"name": "new-state", "state": "UNRECOGNIZED"}], "unknown"),
            ([{"name": "incomplete", "state": "COMPLETED"}], "unknown"),
            ([{"state": "SUCCESS"}, {"state": "UNRECOGNIZED"}], "unknown"),
            ([{"state": "PENDING"}, {"state": "UNRECOGNIZED"}], "unknown"),
        ]:
            with self.subTest(checks=checks):
                exit_code, output = self.invoke(checks)
                self.assertEqual((exit_code, output["status"]), (2, status))

    def test_malformed_or_invalid_checks_json_never_reports_success(self):
        for payload in ["", "not json", "null", "{}", "[null]", '["failure"]', "[1]"]:
            with self.subTest(payload=payload):
                exit_code, output = self.invoke(payload)
                self.assertEqual((exit_code, output["status"]), (2, "error"))
                self.assertTrue(output["error"])

    def test_failed_checks_take_precedence_over_pending(self):
        exit_code, output = self.invoke(
            [self.failed_check(), {"name": "build", "state": "QUEUED"}], gh_exit=1
        )
        self.assertEqual((exit_code, output["status"]), (1, "failed"))
        self.assertEqual(output["counts"]["pending"], 1)

    def test_cancelled_checks_are_failures(self):
        exit_code, output = self.invoke([{"name": "cancelled", "bucket": "cancel"}], gh_exit=1)
        self.assertEqual((exit_code, output["status"]), (1, "failed"))

    def test_field_fallback_accepts_failed_json_exit_one(self):
        unavailable = helper.GhResult(
            1, "", 'Unknown JSON field: "bucket"\nAvailable fields:\n  name\n  state\n  conclusion\n  detailsUrl\n'
        )
        failure = {"name": "legacy", "state": "COMPLETED", "conclusion": "FAILURE"}
        exit_code, output = self.invoke(
            None, responses=[unavailable, helper.GhResult(1, json.dumps([failure]), "")]
        )
        self.assertEqual((exit_code, output["status"]), (1, "failed"))
        self.assertEqual(len(self.calls), 2)
        self.assertEqual(self.calls[1][-1], "name,state,conclusion,detailsUrl")

    def test_field_fallback_accepts_pending_json_exit_eight(self):
        unavailable = helper.GhResult(1, "", "Available fields:\n  name\n  state\n")
        pending = helper.GhResult(8, '[{"name": "build", "state": "PENDING"}]', "")
        exit_code, output = self.invoke(None, responses=[unavailable, pending])
        self.assertEqual((exit_code, output["status"]), (8, "pending"))
        self.assertEqual(len(self.calls), 2)

    def test_nonzero_exit_cannot_turn_terminal_json_into_success(self):
        for gh_exit in (1, 8, 2):
            with self.subTest(gh_exit=gh_exit):
                exit_code, output = self.invoke([{"state": "SUCCESS"}], gh_exit=gh_exit)
                self.assertEqual((exit_code, output["status"]), (2, "error"))

    def test_external_actions_looking_urls_do_not_fetch_github_runs(self):
        for url in [
            "https://ci.example.com/org/repo/actions/runs/123/job/456",
            "https://github.com.example.com/org/repo/actions/runs/123/job/456",
            "https://github.enterprise.example/org/repo/actions/runs/123/job/456",
            "https://github.com@ci.example.com/org/repo/actions/runs/123/job/456",
            "https://ci.example.com/?url=https://github.com/org/repo/actions/runs/123",
            "https://github.com/org/repo/actions/runs/not-a-number/job/456",
        ]:
            with self.subTest(url=url):
                check = self.failed_check()
                check["link"] = url
                exit_code, output = self.invoke([check], gh_exit=1)
                self.assertEqual(exit_code, 1)
                result = output["results"][0]
                self.assertEqual(result["status"], "external")
                self.assertIsNone(result["runId"])
                self.assertIsNone(result["jobId"])
                self.assertEqual(len(self.calls), 1)
                self.assertEqual(self.raw_calls, [])

    def test_identified_job_log_is_preferred_to_run_log(self):
        _, output = self.invoke([self.failed_check()], gh_exit=1)
        self.assertIn("ERROR: job 456", output["results"][0]["logSnippet"])
        self.assertEqual(len(self.raw_calls), 1)
        self.assertFalse(any("--log" in call for call in self.calls))

    def test_duplicate_checks_share_run_metadata_and_run_log(self):
        checks = [self.failed_check("first", job_id=None), self.failed_check("second", job_id=None)]
        _, output = self.invoke(checks, gh_exit=1)
        calls = [call for call in self.calls if call[:2] == ["run", "view"]]
        self.assertEqual(len(calls), 2)
        self.assertEqual(sum("--json" in call for call in calls), 1)
        self.assertEqual(sum("--log" in call for call in calls), 1)
        self.assertEqual(len(output["results"]), 2)
        self.assertTrue(all("ERROR: run failure" in item["logSnippet"] for item in output["results"]))

    def test_duplicate_job_checks_share_metadata_repository_lookup_and_log(self):
        _, output = self.invoke([self.failed_check("first"), self.failed_check("second")], gh_exit=1)
        self.assertEqual(len(output["results"]), 2)
        self.assertEqual(len(self.raw_calls), 1)
        self.assertEqual(sum(call[:2] == ["repo", "view"] for call in self.calls), 1)
        self.assertEqual(sum(call[:2] == ["run", "view"] for call in self.calls), 1)

    def test_distinct_jobs_share_metadata_but_keep_their_own_logs(self):
        _, output = self.invoke(
            [self.failed_check("first", "456"), self.failed_check("second", "789")], gh_exit=1
        )
        self.assertEqual(len(self.raw_calls), 2)
        self.assertEqual(sum(call[:2] == ["repo", "view"] for call in self.calls), 1)
        self.assertEqual(sum(call[:2] == ["run", "view"] for call in self.calls), 1)
        self.assertIn("ERROR: job 456", output["results"][0]["logSnippet"])
        self.assertIn("ERROR: job 789", output["results"][1]["logSnippet"])

    def test_unavailable_job_logs_fall_back_to_one_shared_run_log(self):
        self.job_logs = {"456": (1, b"", "not available"), "789": (1, b"", "not available")}
        _, output = self.invoke(
            [self.failed_check("first", "456"), self.failed_check("second", "789")], gh_exit=1
        )
        self.assertEqual(sum("--log" in call for call in self.calls), 1)
        self.assertTrue(all("ERROR: run failure" in item["logSnippet"] for item in output["results"]))

    def test_pending_logs_do_not_hide_a_failed_check(self):
        self.job_logs = {"456": (1, b"", "log will be available when it is complete")}
        self.run_log = helper.GhResult(1, "", "run is still in progress")
        exit_code, output = self.invoke([self.failed_check()], gh_exit=1)
        self.assertEqual((exit_code, output["status"]), (1, "failed"))
        self.assertEqual(output["results"][0]["status"], "log_pending")

    def test_cache_does_not_survive_an_invocation(self):
        for _ in range(2):
            self.invoke([self.failed_check(job_id=None)], gh_exit=1)
            self.assertEqual(sum(call[:2] == ["run", "view"] for call in self.calls), 2)

    def test_repository_or_process_error_stays_json(self):
        args = argparse.Namespace(repo=".", pr="7", max_lines=80, context=12, json=True)
        for root_result in (None, FileNotFoundError("missing repository")):
            with self.subTest(root_result=root_result):
                stdout = io.StringIO()
                root_mock = (
                    patch.object(helper, "find_git_root", side_effect=root_result)
                    if isinstance(root_result, Exception)
                    else patch.object(helper, "find_git_root", return_value=root_result)
                )
                with (
                    patch.object(helper, "parse_args", return_value=args),
                    root_mock,
                    contextlib.redirect_stdout(stdout),
                ):
                    exit_code = helper.main()
                self.assertEqual(exit_code, 2)
                self.assertEqual(json.loads(stdout.getvalue())["status"], "error")


if __name__ == "__main__":
    unittest.main()
