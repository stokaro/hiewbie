# Source and adaptation

Source: https://github.com/openai/skills
Commit: `49f948faa9258a0c61caceaf225e179651397431`
Path: `skills/.curated/gh-fix-ci/`
License: Apache-2.0; the complete upstream license is retained in `LICENSE.txt`.

The skill entry is adapted to support an already connected GitHub integration,
reuse existing task authorization, bound log output, and preserve head/run/job
provenance. No blanket CLI login or additional planning skill is required.

`scripts/inspect_pr_checks.py` is adapted from the upstream helper: accept valid
check JSON with failure/pending exit codes, distinguish unknown/pending/absent
checks, avoid external URL confusion, prefer failed-job logs, and cache repeated
run requests within one invocation. Bounded JSON output avoids duplicate log
snippets. The helper requires Python 3.10+ and GitHub CLI JSON check support.

Run offline regression tests from this skill directory:

```sh
python3 -m unittest discover -s scripts -p 'test_*.py'
```

These adaptations are not an upstream OpenAI release. Original application code
in hiewbie remains MIT; this bundled development component retains Apache-2.0.
