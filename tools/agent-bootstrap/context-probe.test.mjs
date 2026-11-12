import { test } from 'node:test';
import assert from 'node:assert/strict';
import { withNoteProbe, probeRows, verdictOf } from './context-probe-lib.mjs';

test('probe: blind model reproduces answers and ignores the note', async () => {
  const messages = [{role:'system',content:'s'},{role:'user',content:JSON.stringify({player:1,round:1,allowed_actions:[]})}];
  const blind = async () => ({ content: '{"action":"pass","params":{}}' });
  const rows = await probeRows([messages], blind, ['{"action":"pass","params":{}}']);
  assert.equal(rows[0].control_equal, true);
  assert.equal(rows[0].probe_equal, true);
  assert.equal(verdictOf(rows), 'blind_to_advisory');
});
test('probe: sensitive model changes answer under the note', async () => {
  const messages = [{role:'system',content:'s'},{role:'user',content:JSON.stringify({player:1,round:1,allowed_actions:[]})}];
  const sensitive = async (m) => m[m.length-1].content.includes('advisory_note')
    ? { content: '{"action":"buy","params":{}}' } : { content: '{"action":"pass","params":{}}' };
  const rows = await probeRows([messages], sensitive, ['{"action":"pass","params":{}}']);
  assert.equal(rows[0].probe_equal, false);
  assert.equal(verdictOf(rows), 'sensitive_to_advisory');
});
test('probe: nondeterministic model yields inconclusive control', async () => {
  const messages = [{role:'system',content:'s'},{role:'user',content:'{}'}];
  let n = 0;
  const noisy = async () => ({ content: ++n === 1 ? 'a' : 'b' });
  const rows = await probeRows([messages, messages], noisy, ['a','a']);
  assert.ok(rows.some(r => !r.control_equal));
  assert.equal(verdictOf(rows), 'inconclusive_control_mismatch');
});
test('probe: note injection keeps JSON shape', () => {
  const [m] = withNoteProbe([{role:'user',content:JSON.stringify({player:2})}],'hi');
  const body = JSON.parse(m.content);
  assert.equal(body.advisory_note, 'hi');
  assert.equal(body.player, 2);
});
