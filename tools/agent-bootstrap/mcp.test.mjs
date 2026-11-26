import test from 'node:test';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';
import { fileURLToPath } from 'node:url';

test('MCP stdio: initialize, tools/list, offline tool error, unknown method', async () => {
  const server = spawn(process.execPath, [fileURLToPath(new URL('./mcp.mjs', import.meta.url))], { stdio: ['pipe', 'pipe', 'inherit'] });
  const replies = new Map();
  createInterface({ input: server.stdout }).on('line', l => { const m = JSON.parse(l); replies.get(m.id)?.(m); });
  const call = (id, method, params) => new Promise(ok => { replies.set(id, ok);
    server.stdin.write(JSON.stringify({ jsonrpc: '2.0', id, method, params }) + '\n'); });
  try {
    const init = await call(1, 'initialize', { protocolVersion: '2025-06-18', capabilities: {}, clientInfo: { name: 't', version: '0' } });
    assert.equal(init.result.serverInfo.name, 'alashi');
    server.stdin.write(JSON.stringify({ jsonrpc: '2.0', method: 'notifications/initialized' }) + '\n');
    const list = await call(2, 'tools/list', {});
    assert.deepEqual(list.result.tools.map(t => t.name),
      ['alashi_start', 'alashi_state', 'alashi_act', 'alashi_retry', 'alashi_watch_devnet_game']);
    const watch = list.result.tools.find(t => t.name === 'alashi_watch_devnet_game');
    assert.equal(watch.inputSchema.properties.after.description.slice(0, 15), 'dialogue cursor');
    const bad = await call(3, 'tools/call', { name: 'alashi_watch_devnet_game', arguments: { game_pda: 'not a pda' } });
    assert.equal(bad.result.isError, true);
    assert.match(bad.result.content[0].text, /invalid_game/);
    // Output discipline: single-line JSON, head marker first, honest byte count.
    const text = bad.result.content[0].text;
    assert.ok(!text.includes('\n'), 'tool text must be single-line');
    assert.ok(text.startsWith('{"mcp_payload":'), 'mcp_payload must be the first key');
    const marked = JSON.parse(text);
    assert.equal(marked.mcp_payload.complete, true);
    assert.ok(Number.isInteger(marked.mcp_payload.bytes) && marked.mcp_payload.bytes > 0);
    assert.equal(marked.ok, false); // error body preserved after the marker
    const unknown = await call(4, 'nope', {});
    assert.equal(unknown.error.code, -32601);
  } finally { server.kill(); }
});
