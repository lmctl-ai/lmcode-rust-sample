#!/usr/bin/env python3
"""Recompile with the selected lmscript; never rewrite the checked-in oracle."""
from pathlib import Path
import subprocess
import sys
import tempfile
root=Path(__file__).resolve().parent.parent
with tempfile.TemporaryDirectory(prefix='sample-recompile-') as temporary:
    out=Path(temporary)
    subprocess.run([sys.executable,str(root/'sample-suite/compile.py'),'--output',str(out)],check=True)
    expected=list((root/'sample-suite/bytecode').glob('*.dslb'))
    if {p.name for p in out.glob('*.dslb')}!={p.name for p in expected}:
        sys.exit('compiler produced a different case set')
    for source in expected:
        if source.read_bytes()!=(out/source.name).read_bytes():
            sys.exit('compiler bytecode drift: '+source.name)
print('PASS compiler reproduces all pinned bytecode')
