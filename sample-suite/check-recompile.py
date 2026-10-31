#!/usr/bin/env python3
"""Recompile pinned bytes, explicitly reporting unsupported released features."""
from pathlib import Path
import json
import os
import runpy
import shutil
import hashlib
import subprocess
import sys
import tempfile
suite = Path(__file__).resolve().parent
manifest = json.loads((suite / 'manifest.json').read_text())
runpy.run_path(str(suite / 'run-suite.py'))['check_files'](manifest)
compiler = shutil.which(os.environ.get('LMSCRIPT_BIN', 'lmscript'))
if not compiler:
    sys.exit('lmscript compiler unavailable')
features = {f for case in manifest['cases'] for f in case.get('compiler_features', [])}
flagged = [case for case in manifest['cases'] if case.get('compiler_features')]
excluded = set()
for case in flagged:
    probe = subprocess.run([compiler, '--check', str(suite / case['source'])], capture_output=True, text=True)
    if probe.returncode:
        with tempfile.TemporaryDirectory(prefix='compiler-capability-') as probe_dir:
            schema = Path(probe_dir) / 'empty.json'
            schema.write_text('{"dsl.schema":2,"name":"probe"}')
            surface = subprocess.run([compiler, '--lmcode-signatures', str(schema)], capture_output=True)
        legacy = surface.returncode == 0 and hashlib.sha256(surface.stdout.replace(b'\r\n', b'\n')).hexdigest() == "ad2341a497c8ece5e09ca17dc04c8b8ad28c04abe18d1e48769f3e04e8b90c2f"
        if (legacy and features == {'lmcode-literals-v1'}
                and os.environ.get('LMCODE_REQUIRE_LITERAL_COMPILER') != '1'):
            excluded = features
            break
        sys.exit('literal-capable compiler rejected pinned source: ' + probe.stderr)
with tempfile.TemporaryDirectory(prefix='sample-recompile-') as temporary:
    out = Path(temporary)
    args = [sys.executable, str(suite / 'compile.py'), '--output', str(out)]
    for feature in sorted(excluded):
        args += ['--exclude-feature', feature]
    subprocess.run(args, check=True)
    expected = [case for case in manifest['cases'] if not set(case.get('compiler_features', [])) & excluded]
    if {p.name for p in out.glob('*.dslb')} != {Path(case['bytecode']).name for case in expected}:
        sys.exit('compiler produced a different case set')
    for case in expected:
        source = suite / case['bytecode']
        if source.read_bytes() != (out / source.name).read_bytes():
            sys.exit('compiler bytecode drift: ' + source.name)
if excluded:
    print(f'SKIP {len(flagged)} literal source reproductions: verified v1 compiler lacks lmcode-literals-v1; use LMSCRIPT_BIN or LMCODE_REQUIRE_LITERAL_COMPILER=1')
print(f'PASS compiler reproduces {len(expected)}/{len(manifest["cases"])} pinned bytecode cases')
