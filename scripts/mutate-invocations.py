#!/usr/bin/env python3
"""Compile sample adapter faults and require the real public builder flow to reject them."""
from pathlib import Path
import os
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
PYTHON = sys.executable
CHECK = ROOT / "sample-suite/check-invocations.py"
LANG = "rust" if (ROOT / "Cargo.toml").exists() else "go"
SOURCE = ROOT / ("src/main.rs" if LANG == "rust" else "main.go")
IGNORE = shutil.ignore_patterns(".git", "target", "build", "__pycache__")

with tempfile.TemporaryDirectory(prefix=f"{LANG}-sample-invocation-mutants-") as temporary:
    work = Path(temporary) / "repo"
    shutil.copytree(ROOT, work, ignore=IGNORE)
    source = work / SOURCE.relative_to(ROOT)
    original = source.read_text()
    env = os.environ.copy()
    host = work / ("target/release/lmcode-rust-sample" if LANG == "rust" else "build/lmcode-go-sample")

    def build():
        if LANG == "rust":
            command = ["cargo", "build", "--release", "--locked", "--offline"]
        else:
            (work / "build").mkdir(exist_ok=True)
            command = ["go", "build", "-trimpath", "-o", str(host), "."]
        result = subprocess.run(command, cwd=work, env=env, capture_output=True, text=True)
        if result.returncode:
            raise AssertionError(("mutant build failed", result.stdout, result.stderr))

    def check(label):
        if label == "stale bare frame":
            fixture = work / "sample-suite/bytecode/068-rule-entries.dslb"
            raw = subprocess.run([str(host), "--lmcode", str(fixture)], cwd=work, capture_output=True)
            if raw.returncode != 0:
                raise AssertionError((label, "mutant host failed", raw.returncode, raw.stdout, raw.stderr))
            lines = raw.stdout.splitlines()
            begins = [i for i, line in enumerate(lines) if line == b"LMCODE_RETURN_BEGIN"]
            ends = [i for i, line in enumerate(lines) if line == b"LMCODE_RETURN_END"]
            bare = b'{"ok":true,"result":null}'
            if len(begins) != 1 or len(ends) != 1 or ends[0] != begins[0] + 2:
                raise AssertionError((label, "terminal frame shape", raw.stdout))
            if not any(line == bare for i, line in enumerate(lines) if i < begins[0] or i > ends[0]):
                raise AssertionError((label, "mutated bare frame not observed", raw.stdout))
            print(f"CAUGHT {label} by raw host frame oracle", flush=True)
            return
        result = subprocess.run([PYTHON, str(CHECK), str(host)], cwd=work, env=env, capture_output=True, text=True)
        if result.returncode != 1 or "AssertionError" not in result.stderr:
            raise AssertionError((label, result.returncode, result.stdout, result.stderr))
        print(f"CAUGHT {label}", flush=True)

    build()
    baseline = subprocess.run([PYTHON, str(CHECK), str(host)], cwd=work, env=env, capture_output=True, text=True)
    if baseline.returncode:
        raise AssertionError(("mutation baseline failed", baseline.stdout, baseline.stderr))
    print("PASS invocation mutation baseline", flush=True)

    if LANG == "rust":
        mutations = [
            ("reset shared host counter", "let part = results.registrations()[position].clone();", "{ let ledger = host.context.get_mut::<shop::Ledger>(host.ledger).unwrap(); ledger.balance = 0; ledger.entries = 0; }\n        let part = results.registrations()[position].clone();"),
            ("stale bare frame", "    Ok(results.write_frame(&mut io::stdout().lock()))", "    println!(\"{{\\\"ok\\\":true,\\\"result\\\":null}}\");\n    Ok(results.write_frame(&mut io::stdout().lock()))"),
            ("duplicate part", "let mut execution: Vec<usize> = (0..files.len()).collect();", "let mut execution: Vec<usize> = vec![0, 0];"),
            ("missing part", "for position in execution {", "execution.truncate(1);\n    for position in execution {"),
            ("argv role loss", '"--first" => 0,\n            "--last" => 2,', '"--first" => 1,\n            "--last" => 1,'),
        ]
    else:
        mutations = [
            ("reset shared host counter", "for _, part := range parts {", "for _, part := range parts {\n\t\thost.Ledger.Balance = 0\n\t\thost.Ledger.Entries = 0"),
            ("stale bare frame", "\treturn results.WriteFrame(os.Stdout)", "\tfmt.Println(\"{\\\"ok\\\":true,\\\"result\\\":null}\")\n\treturn results.WriteFrame(os.Stdout)"),
            ("duplicate part", "sort.SliceStable(parts, func(i, j int) bool { return parts[i].priority < parts[j].priority })", "sort.SliceStable(parts, func(i, j int) bool { return parts[i].priority < parts[j].priority })\n\tparts = []invocationArg{parts[0], parts[0]}"),
            ("missing part", "for _, part := range parts {", "parts = parts[:1]\n\tfor _, part := range parts {"),
            ("argv role loss", 'case "--first":\n\t\t\tpriority = 0\n\t\tcase "--last":\n\t\t\tpriority = 2', 'case "--first":\n\t\t\tpriority = 1\n\t\tcase "--last":\n\t\t\tpriority = 1'),
        ]

    for label, anchor, replacement in mutations:
        if original.count(anchor) != 1:
            raise AssertionError((label, "mutation anchor count", original.count(anchor)))
        source.write_text(original.replace(anchor, replacement))
        build()
        check(label)
        source.write_text(original)
    build()
