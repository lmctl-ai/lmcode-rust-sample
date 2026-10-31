"""Independent terminal-frame oracle, also used before recording expectations."""
import json
BEGIN=b'LMCODE_RETURN_BEGIN\n'
END=b'LMCODE_RETURN_END\n'

def validate(stdout, code, expected=None):
    lines=stdout.splitlines(keepends=True)
    if code:
        if BEGIN in lines or END in lines:
            raise ValueError('failed invocation emitted a return frame')
        return
    if len(lines)<4 or lines[-3]!=BEGIN or lines[-1]!=END or lines.count(BEGIN)!=1 or lines.count(END)!=1:
        raise ValueError('missing, duplicate or nonterminal return frame')
    if not lines[-2].endswith(b'\n') or b'\r' in b''.join(lines[-3:]):
        raise ValueError('return frame must use LF bytes')
    payload=json.loads(lines[-2].decode('utf8'))
    if json.dumps(payload, sort_keys=True) != json.dumps(expected, sort_keys=True):
        raise ValueError('invocation result differs from independent return oracle')
    diagnostic=json.loads(lines[-4])
    if diagnostic.get('ok') is not True:
        raise ValueError('success diagnostic must precede frame')
