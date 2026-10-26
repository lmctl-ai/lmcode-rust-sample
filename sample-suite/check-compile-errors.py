#!/usr/bin/env python3
"""Compile-only language features: recursion is rejected, not silently emulated."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
ROOT = Path(__file__).resolve().parent

def check():
    compiler = shutil.which(os.environ.get('LMSCRIPT_BIN', 'lmscript'))
    if not compiler:
        raise ValueError('lmscript is required for compile-rejection cases')
    cases = json.loads((ROOT / 'compile-errors/manifest.json').read_text())['cases']
    for case in cases:
        with tempfile.TemporaryDirectory(prefix='sample-compile-error-') as work:
            output = Path(work) / 'must-not-exist.dslb'
            result = subprocess.run([compiler, '--no-preflight', str(ROOT/'compile-errors'/case['source']), str(output)], capture_output=True, timeout=30)
            if result.returncode != case['exit'] or case['diagnostic'].encode() not in result.stderr or output.exists():
                raise ValueError('compiler rejection mismatch: ' + case['source'] + ': ' + result.stderr.decode(errors='replace'))
    print(f'PASS {len(cases)} compiler rejection case(s), exit 2')

if __name__ == '__main__':
    try:
        check()
    except (OSError, ValueError, subprocess.TimeoutExpired) as error:
        sys.exit(str(error))
