#!/usr/bin/env bash
set -euo pipefail
suite_dir="$(cd "$(dirname "$0")" && pwd)"
exec python3 "$suite_dir/run-suite.py" "$@"
