#!/usr/bin/env python3
"""Offline H6 sentinel: dry skips writes, real file reads run once per call, real stays unchanged."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

SUITE = Path(__file__).resolve().parents[1] / "sample-suite"


def invoke(host, bytecode, scratch, dry):
    manifest = scratch / "invocation.json"
    manifest.write_text(json.dumps({
        "version": 1,
        "mode": "single",
        "dry": dry,
        "parts": [{"index": 0, "file": str(bytecode.resolve())}],
    }))
    env = dict(os.environ, SAMPLE_TMP=str(scratch), LMCODE_INVOCATION_FILE=str(manifest))
    result = subprocess.run([host, "--lmcode", str(bytecode)], env=env, capture_output=True)
    if result.stderr:
        raise AssertionError((result.returncode, result.stdout, result.stderr))
    frame = result.stdout.splitlines()[-2]
    return result, json.loads(frame)


def check(host):
    bytecode = SUITE / "bytecode/024-file-functions.dslb"
    with tempfile.TemporaryDirectory(prefix="lmcode-dry-sentinel-") as temp:
        scratch = Path(temp)
        sentinel = scratch / "ledger.txt"
        sentinel.write_text("seed")
        dry_result, dry_frame = invoke(host, bytecode, scratch, True)
        if dry_result.returncode != 0 or dry_frame != {"ok": True, "result": None, "dry": True}:
            raise AssertionError((dry_result.returncode, dry_frame, dry_result.stdout))
        if sentinel.read_text() != "seed":
            raise AssertionError("dry file_write/file_append modified the sentinel")
        if b"true:seed:1" not in dry_result.stdout:
            raise AssertionError(("dry(real) reads did not observe the fixture", dry_result.stdout))

        control, control_frame = invoke(host, bytecode, scratch, False)
        if control.returncode != 0 or control_frame != {"ok": True, "result": None} or sentinel.read_text() != "onetwo":
            raise AssertionError(("real-mode sentinel control did not exercise writes", control.returncode, control_frame, sentinel.read_text()))
        sentinel.write_text("seed")

        real_env = dict(os.environ, SAMPLE_TMP=str(scratch))
        real_env.pop("LMCODE_INVOCATION_FILE", None)
        real = subprocess.run([host, "--lmcode", str(bytecode)], env=real_env, capture_output=True)
        if real.returncode != 0 or real.stderr or sentinel.read_text() != "onetwo":
            raise AssertionError((real.returncode, real.stdout, real.stderr, sentinel.read_text()))
        if b"true:onetwo:1" not in real.stdout:
            raise AssertionError(("real output changed", real.stdout))

        setup = invoke(host, SUITE / "bytecode/068-rule-entries.dslb", scratch, True)
        if setup[0].returncode != 0 or setup[1].get("dry") is not True:
            raise AssertionError(("dry setup invocation failed", setup[0].returncode, setup[1]))
        summaries = [line for line in setup[0].stdout.splitlines() if b'"host_summary"' in line]
        if len(summaries) != 1 or b'"entries_called": 1' not in summaries[0] or b"total rule 0" not in setup[0].stdout:
            raise AssertionError(("dry setup exposed seeded live inventory", setup[0].stdout))
    print("PASS dry sentinel suppression, dry(real) read, and unchanged real write path")


if __name__ == "__main__":
    check(str(Path(sys.argv[1]).resolve()))
