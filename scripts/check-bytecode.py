#!/usr/bin/env python3
"""Use the canonical corpus reproduction policy, including explicit v1 skips."""
from pathlib import Path
import runpy
runpy.run_path(str(Path(__file__).resolve().parent.parent / 'sample-suite/check-recompile.py'), run_name='__main__')
