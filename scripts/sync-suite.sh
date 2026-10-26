#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
python3 "$root/scripts/sync.py" suite "${1:-$root/../lmcode-cpp}" "${2:-HEAD}"
