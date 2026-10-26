#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "$0")" && pwd)"
cd "$root"
python3 -B scripts/sync.py check
cargo build --release --locked
python3 -B sample-suite/audit.py --std vendor/lmcode-rust/std/lmcode-std.schema.json
python3 -B scripts/check-bytecode.py
host="target/release/lmcode-rust-sample"
[[ -x "$host" ]] || host="target/release/lmcode-rust-sample.exe"
python3 -B sample-suite/run-suite.py "$host"
python3 -B sample-suite/test_runner.py
