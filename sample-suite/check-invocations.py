#!/usr/bin/env python3
"""Exercise public builders and enforce one terminal envelope on the native adapter."""
import json
import os
import shlex
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent

def check(host):
    host = Path(host).resolve()
    with tempfile.TemporaryDirectory(prefix="sample-invocation-") as tmp:
        source = Path(tmp) / "real.lms"
        script = (ROOT / "invocations.lms").read_text().replace('"__HOST__"', json.dumps(shlex.quote(str(host))))
        source.write_text(script)
        result = subprocess.run([os.environ.get("LMSCRIPT_BIN", "lmscript"), "--no-preflight", str(source), str(host)], capture_output=True)
    if result.returncode != 0 or b"PASS real single/builder" not in result.stdout:
        raise AssertionError((result.returncode, result.stdout, result.stderr))
    if result.stdout.count(b'"host_summary":') != 8:
        raise AssertionError(("lost summary", result.stdout))

    direct = subprocess.run([str(host), "--lmcode", str(ROOT / "bytecode/068-rule-entries.dslb")], capture_output=True)
    if direct.returncode != 0 or direct.stderr:
        raise AssertionError(("direct single host run", direct.returncode, direct.stdout, direct.stderr))
    if direct.stdout.count(b"LMCODE_RETURN_BEGIN\n") != 1 or direct.stdout.count(b"LMCODE_RETURN_END\n") != 1:
        raise AssertionError(("frame count", direct.stdout))
    if not direct.stdout.endswith(b"LMCODE_RETURN_END\n"):
        raise AssertionError(("frame is not terminal", direct.stdout))
    outside = direct.stdout.split(b"LMCODE_RETURN_BEGIN\n", 1)[0]
    if any(line.lstrip().startswith(b'{"ok"') for line in outside.splitlines()):
        raise AssertionError(("stale bare frame", outside))
    frame = direct.stdout.split(b"LMCODE_RETURN_BEGIN\n", 1)[1].split(b"\nLMCODE_RETURN_END\n", 1)[0]
    if json.loads(frame) != {"ok": True, "result": None}:
        raise AssertionError(("single envelope", frame))
    print("PASS public single/builder integration, terminal frame, and eight summaries")

if __name__ == "__main__":
    check(sys.argv[1])
