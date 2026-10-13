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
    const bad = await call(3, 'tools/call', { name: 'alashi_watch_devnet_game', arguments: { game_pda: 'not a pda' } });
    assert.equal(bad.result.isError, true);
    assert.match(bad.result.content[0].text, /invalid_game/);
    const unknown = await call(4, 'nope', {});
    assert.equal(unknown.error.code, -32601);
  } finally { server.kill(); }
});
