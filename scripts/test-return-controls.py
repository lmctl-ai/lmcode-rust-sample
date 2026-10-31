#!/usr/bin/env python3
"""Compile frame and entry-sink faults against the canonical return oracle."""
import json, os, runpy, shutil, subprocess, tempfile
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]; SUITE=ROOT/'sample-suite'
validate=runpy.run_path(str(SUITE/'return_protocol.py'))['validate']
cases={c['name']:c for c in json.loads((SUITE/'manifest.json').read_text())['cases']}
source=(ROOT/'src/main.rs').read_text()
variants=[
 ('baseline',source,None),
 ('missing-frame',source.replace('    Ok(results.write_frame(&mut io::stdout().lock()))','    Ok(0)'), '001-literals'),
 ('reset-between-entries',source.replace('let value =\n                        program.call(index, &mut host.context, &host.registry, vec![argument])?;','host.context.result_sink = Some(dslvm::stdlib::ResultSink::default());\n                    let value =\n                        program.call(index, &mut host.context, &host.registry, vec![argument])?;'),'364-return-entry-lifetime'),
]
for name,changed,target in variants:
 if target and changed==source: raise AssertionError('mutation anchor missing: '+name)
 with tempfile.TemporaryDirectory(prefix='rust-return-control-') as temp:
  root=Path(temp)
  for file in ['Cargo.toml','Cargo.lock','build.rs']: shutil.copy2(ROOT/file,root/file)
  shutil.copytree(ROOT/'src',root/'src'); shutil.copytree(ROOT/'vendor',root/'vendor')
  (root/'sample-suite').mkdir(); shutil.copy2(SUITE/'sample.dsl.json',root/'sample-suite/sample.dsl.json')
  (root/'src/main.rs').write_text(changed)
  subprocess.run(['cargo','build','--release','--locked','--offline'],cwd=root,check=True,capture_output=True)
  binary=root/'target/release'/('lmcode-rust-sample.exe' if os.name=='nt' else 'lmcode-rust-sample')
  for case_name in ([target] if target else ['001-literals','364-return-entry-lifetime']):
   case=cases[case_name]
   result=subprocess.run([str(binary),'--lmcode',str(SUITE/case['bytecode'])],cwd=root,env=dict(os.environ,SAMPLE_TMP=temp),capture_output=True)
   try: validate(result.stdout,result.returncode,case.get('expected_return'))
   except ValueError:
    if not target: raise
    print('CAUGHT '+name)
   else:
    if target: raise AssertionError('mutation survived: '+name)
    assert result.returncode==0 and result.stderr==b'',(name,result.returncode,result.stderr)
    assert result.stdout==(SUITE/'expected'/(case_name+'.out')).read_bytes()
  if not target: print('PASS baseline')
