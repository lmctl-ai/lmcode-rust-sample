#!/usr/bin/env python3
"""Compile real host faults; compile errors and unexpected exits are not controls."""
import json
import os
from pathlib import Path
import runpy
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
SUITE = ROOT / 'sample-suite'
validate = runpy.run_path(str(SUITE / 'return_protocol.py'))['validate']
cases = {c['name']: c for c in json.loads((SUITE / 'manifest.json').read_text())['cases']}
source = (ROOT / 'src/main.rs').read_text()
variants = [
    ('baseline', source, None),
    ('missing-frame', source.replace('    out.write_all(frame.as_bytes())?;', '    let _ = frame;'), '001-literals'),
    ('reset-between-entries', source.replace('            let value = match program.call(', '            host.context.result_sink.as_mut().unwrap().reset();\n            let value = match program.call('), '364-return-entry-lifetime'),
]
for name, changed, target in variants:
    if target and changed == source:
        raise AssertionError('mutation anchor missing: ' + name)
    with tempfile.TemporaryDirectory(prefix='rust-return-control-') as temp:
        root = Path(temp)
        for file in ['Cargo.toml', 'Cargo.lock', 'build.rs']:
            shutil.copy2(ROOT / file, root / file)
        shutil.copytree(ROOT / 'src', root / 'src')
        shutil.copytree(ROOT / 'vendor', root / 'vendor')
        (root / 'sample-suite').mkdir()
        shutil.copy2(SUITE / 'sample.dsl.json', root / 'sample-suite/sample.dsl.json')
        (root / 'src/main.rs').write_text(changed)
        subprocess.run(['cargo', 'build', '--release', '--locked', '--offline'], cwd=root, check=True, capture_output=True)
        binary = root / 'target/release' / ('lmcode-rust-sample.exe' if os.name == 'nt' else 'lmcode-rust-sample')
        for case_name in ([target] if target else ['001-literals', '364-return-entry-lifetime']):
            case = cases[case_name]
            result = subprocess.run([str(binary), '--dsl', str(SUITE / case['bytecode'])], cwd=root, env=dict(os.environ, SAMPLE_TMP=temp), capture_output=True)
            assert result.returncode == 0 and result.stderr == b'', (name, result.returncode, result.stderr)
            try:
                validate(result.stdout, result.returncode, case.get('expected_return'))
            except ValueError:
                if not target:
                    raise
                print('CAUGHT ' + name)
            else:
                if target:
                    raise AssertionError('mutation survived: ' + name)
                assert result.stdout == (SUITE / 'expected' / (case_name + '.out')).read_bytes()
        if not target:
            print('PASS baseline')
