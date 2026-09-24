#!/usr/bin/env python3
"""Require successful GitHub CI for the exact ref before mirroring it."""

import argparse
import json
import os
import subprocess


def master_ready(sha: str, current_sha: str, runs: dict[str, list[dict]]) -> bool:
    return sha == current_sha and all(
        entries
        and entries[0].get("head_sha") == sha
        and entries[0].get("event") == "push"
        and entries[0].get("conclusion") == "success"
        for entries in (runs.get("rust.yml", []), runs.get("docs.yml", []))
    )


def release_ready(tag: str, sha: str, current_sha: str, run: dict) -> bool:
    return (
        sha == current_sha
        and run.get("head_sha") == sha
        and (
            run.get("head_branch") == tag
            or run.get("event") == "workflow_dispatch"
        )
        and run.get("conclusion") == "success"
        and any(
            job.get("name") == "Publish GitHub release"
            and job.get("conclusion") == "success"
            for job in run.get("jobs", [])
        )
    )


def _output(*args: str) -> str:
    return subprocess.run(args, check=True, capture_output=True, text=True).stdout.strip()


def _api(path: str) -> dict:
    return json.loads(_output("gh", "api", path))


def _ref_sha(repo: str, ref: str) -> str:
    lines = _output("git", "ls-remote", f"https://github.com/{repo}.git", ref).splitlines()
    if not lines and ref.endswith("^{}"):
        lines = _output(
            "git", "ls-remote", f"https://github.com/{repo}.git", ref[:-3]
        ).splitlines()
    return lines[0].split()[0] if lines else ""


def _runs(repo: str, workflow: str, sha: str) -> list[dict]:
    result = _api(
        f"repos/{repo}/actions/workflows/{workflow}/runs"
        f"?head_sha={sha}&event=push&per_page=20"
    )
    return [
        run
        for run in result.get("workflow_runs", [])
        if run.get("head_sha") == sha and run.get("head_branch") == "master"
    ]


def _release_run(repo: str, tag: str, sha: str, run_id: str | None) -> dict:
    if run_id:
        run = _api(f"repos/{repo}/actions/runs/{run_id}")
    else:
        runs = _api(
            f"repos/{repo}/actions/workflows/release.yml/runs"
            f"?head_sha={sha}&per_page=20"
        ).get("workflow_runs", [])
        run = next(
            (
                item
                for item in runs
                if item.get("head_sha") == sha
                and (item.get("head_branch") == tag or item.get("event") == "workflow_dispatch")
            ),
            {},
        )
    if run.get("id"):
        run["jobs"] = _api(
            f"repos/{repo}/actions/runs/{run['id']}/jobs?per_page=100"
        ).get("jobs", [])
    return run


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("master", "release"))
    parser.add_argument("--repo", required=True)
    parser.add_argument("--sha")
    parser.add_argument("--tag")
    parser.add_argument("--run-id")
    args = parser.parse_args()

    if args.mode == "master":
        if not args.sha:
            parser.error("--sha is required for master")
        sha = args.sha
        current = _ref_sha(args.repo, "refs/heads/master")
        runs = {
            workflow: _runs(args.repo, workflow, sha)
            for workflow in ("rust.yml", "docs.yml")
        }
        ready = master_ready(sha, current, runs)
    else:
        if not args.tag:
            parser.error("--tag is required for a release")
        current = _ref_sha(args.repo, f"refs/tags/{args.tag}^{{}}")
        sha = args.sha or current
        run = _release_run(args.repo, args.tag, sha, args.run_id)
        ready = release_ready(args.tag, sha, current, run)

    value = "true" if ready else "false"
    if os.environ.get("GITHUB_OUTPUT"):
        with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as handle:
            handle.write(f"ready={value}\n")
            if ready:
                handle.write(f"sha={sha}\n")
    print(f"GitCode {args.mode} mirror gate: {value}")
    if args.mode == "release" and not ready:
        raise SystemExit("GitHub did not successfully publish this tag and commit")


if __name__ == "__main__":
    main()
