#!/usr/bin/env python3
"""Vendor the reviewed Rust SDK and canonical C++ corpus; lock every byte."""
from __future__ import annotations
import hashlib, json, shutil, subprocess, sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

def files(root: Path):
    return sorted(p for p in root.rglob('*') if p.is_file() and '__pycache__' not in p.parts and p.suffix != '.pyc')

def hashes(root: Path):
    return {p.relative_to(root).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest() for p in files(root)}

def source_dir(arg: str | None, default: Path):
    return Path(arg).resolve() if arg else default

def main():
    mode = sys.argv[1] if len(sys.argv) > 1 else ''
    if mode not in {'sdk', 'suite', 'check'}:
        raise SystemExit('usage: sync.py sdk|suite [SOURCE] [REF] | check')
    sdk = ROOT / 'vendor/lmcode-rust'
    suite = ROOT / 'sample-suite'
    if mode == 'check':
        for name, path in [('sdk', sdk), ('suite', suite)]:
            lock = json.loads((ROOT / f'{name}.lock').read_text())
            if hashes(path) != lock['sha256']:
                raise SystemExit(f'{name} drift: run scripts/sync.py {name} against the reviewed source')
        print('PASS SDK and suite locks')
        return
    if mode == 'sdk':
        source_root = source_dir(sys.argv[2] if len(sys.argv) > 2 else None, ROOT.parent / 'lmcode-rust')
        src = source_root / 'dslvm'
        if not (src / 'Cargo.toml').is_file(): raise SystemExit(f'missing dslvm crate: {src}')
        if (ROOT / 'vendor/lmcode-rust').exists(): shutil.rmtree(ROOT / 'vendor/lmcode-rust')
        sdk.mkdir(parents=True)
        commit = subprocess.check_output(['git','-C',str(source_root),'rev-parse',sys.argv[3] if len(sys.argv) > 3 else 'HEAD'], text=True).strip()
        archive = subprocess.Popen(['git','-C',str(source_root),'archive',commit,'dslvm','std','LICENSE'], stdout=subprocess.PIPE)
        assert archive.stdout is not None
        subprocess.run(['tar','-x','-C',str(sdk)], stdin=archive.stdout, check=True)
        archive.wait()
        if archive.returncode: raise SystemExit('failed to archive committed Rust SDK')
        name = 'sdk'
    else:
        source_root = source_dir(sys.argv[2] if len(sys.argv) > 2 else None, ROOT.parent / 'lmcode-cpp')
        commit = subprocess.check_output(['git','-C',str(source_root),'rev-parse',sys.argv[3] if len(sys.argv) > 3 else 'HEAD'], text=True).strip()
        if suite.exists(): shutil.rmtree(suite)
        suite.mkdir(parents=True)
        archive = subprocess.Popen(['git','-C',str(source_root),'archive',commit,'sample-suite'], stdout=subprocess.PIPE)
        assert archive.stdout is not None
        subprocess.run(['tar','-x','--strip-components=1','-C',str(suite)], stdin=archive.stdout, check=True)
        archive.wait()
        if archive.returncode: raise SystemExit('failed to archive canonical suite')
        name = 'suite'
    (ROOT / f'{name}.lock').write_text(json.dumps({'repo': 'lmctl-ai/lmcode-rust' if name == 'sdk' else 'lmctl-ai/lmcode-cpp', 'commit': commit, 'sha256': hashes(sdk if name == 'sdk' else suite)}, indent=2) + '\n')
    print(f'Pinned {name} at {commit}')

if __name__ == '__main__': main()
