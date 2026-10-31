// Independent assertion before a reference output can become an expectation.
export function validateReturn(hex, code, expected = null) {
  const bytes = Buffer.from(hex, 'hex');
  const text = new TextDecoder('utf-8', {fatal:true}).decode(bytes);
  const lines = text.split('\n');
  const count = marker => lines.filter(line => line === marker).length;
  if (code !== 0) {
    if (count('LMCODE_RETURN_BEGIN') || count('LMCODE_RETURN_END')) throw Error('failed invocation emitted return frame');
    return true;
  }
  if (lines.length < 5 || lines.at(-1) !== '' || lines.at(-2) !== 'LMCODE_RETURN_END' || lines.at(-4) !== 'LMCODE_RETURN_BEGIN' || count('LMCODE_RETURN_BEGIN') !== 1 || count('LMCODE_RETURN_END') !== 1)
    throw Error('missing, duplicate or nonterminal return frame');
  const actual = JSON.parse(lines.at(-3));
  if (JSON.stringify(actual) !== JSON.stringify(expected)) throw Error('result differs from independent return oracle');
  if (JSON.parse(lines.at(-5)).ok !== true) throw Error('success diagnostic must precede frame');
  return true;
}
