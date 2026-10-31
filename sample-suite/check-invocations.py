#!/usr/bin/env python3
"""Real public compiler-to-native integration; summaries are separate diagnostics."""
import os,subprocess,sys,json,shlex,tempfile
from pathlib import Path
ROOT=Path(__file__).resolve().parent
def check(host):
 with tempfile.TemporaryDirectory(prefix='sample-invocation-') as tmp:
  source=Path(tmp)/'real.lms';source.write_text((ROOT/'invocations.lms').read_text().replace('\"__HOST__\"',json.dumps(shlex.quote(str(Path(host).resolve())))))
  p=subprocess.run([os.environ.get('LMSCRIPT_BIN','lmscript'),'--no-preflight',str(source),str(Path(host).resolve())],capture_output=True)
 if p.returncode!=0 or b'PASS real single/builder' not in p.stdout:raise AssertionError((p.returncode,p.stdout,p.stderr))
 if p.stdout.count(b'"host_summary":')!=8:raise AssertionError(('lost summary',p.stdout))
 print('PASS public single/builder native integration and eight retained summaries')
if __name__=='__main__':check(sys.argv[1])
