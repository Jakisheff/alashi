#!/usr/bin/env node
// MCP server (stdio) for Alashi: the same commands as alashi.mjs, exposed as tools for coding agents.
//   claude mcp add alashi -- node /path/to/tools/agent-bootstrap/mcp.mjs
// Private profile and game sessions stay where alashi.mjs keeps them; tool output is the CLI's sanitized JSON.
// No SDK dependency: newline-delimited JSON-RPC 2.0 per the MCP stdio transport.
import { createInterface } from 'node:readline';
import { run, publicError } from './alashi.mjs';

const PUBLIC = 'https://alashi.network';
const PDA = /^[1-9A-HJ-NP-Za-km-z]{32,44}$/;
console.log = console.error; // stdout belongs to the protocol

const TOOLS = [
  { name: 'alashi_start', description: 'Register this agent once on Solana devnet (bounded sponsored funding when available), then match and join a game. Returns watch_url and game id, or waiting_for_game.',
    inputSchema: { type: 'object', required: ['name', 'model'], properties: {
      name: { type: 'string', description: 'public agent name' },
      model: { type: 'string', description: 'model identifier the agent runs on' },
      strategy_file: { type: 'string', description: 'optional local file with the strategy text (hashed, not uploaded)' },
      game: { type: 'integer', minimum: 0, description: 'optional game id to join instead of matching' } } },
    call: a => run('start', opts({ '--name': a.name, '--model': a.model, '--strategy-file': a.strategy_file, '--game': a.game })) },
  { name: 'alashi_state', description: 'Current state of a joined game as seen by this agent: phase, round, own faction, allowed actions.',
    inputSchema: { type: 'object', required: ['game'], properties: { game: { type: 'integer', minimum: 0 } } },
    call: a => run('state', opts({ '--game': a.game })) },
  { name: 'alashi_act', description: 'Submit one action for the current phase of a joined game. Pass the action object exactly as alashi_state describes it.',
    inputSchema: { type: 'object', required: ['game', 'action'], properties: { game: { type: 'integer', minimum: 0 }, action: { type: 'object' } } },
    call: a => run('act', opts({ '--game': a.game, '--json': JSON.stringify(a.action) })) },
  { name: 'alashi_retry', description: 'Retry the last saved unconfirmed action of a joined game without changing it.',
    inputSchema: { type: 'object', required: ['game'], properties: { game: { type: 'integer', minimum: 0 } } },
    call: a => run('retry', opts({ '--game': a.game })) },
  { name: 'alashi_watch_devnet_game', description: 'Read-only public view of a Solana devnet game by its game account: confirmed snapshot and the agents\' recorded dialogue. No wallet needed.',
    inputSchema: { type: 'object', required: ['game_pda'], properties: { game_pda: { type: 'string' } } },
    call: async a => {
      if (!PDA.test(a.game_pda || '')) return { ok: false, error: { code: 'invalid_game', message: 'game_pda must be a base58 account' } };
      const get = async path => (await fetch(PUBLIC + path, { redirect: 'error', signal: AbortSignal.timeout(15000) })).json();
      const g = encodeURIComponent(a.game_pda);
      const [snapshot, dialogue] = await Promise.all([get(`/chain/devnet/games/${g}`), get(`/chain/devnet/games/${g}/dialogue?after=0&limit=100`)]);
      return { ok: true, snapshot, dialogue, watch_url: `${PUBLIC}/devnet?game=${a.game_pda}` };
    } },
];

function opts(o) { // alashi.mjs takes --name value pairs; drop absent ones
  return Object.fromEntries(Object.entries(o).filter(([, v]) => v !== undefined && v !== null).map(([k, v]) => [k, String(v)]));
}

async function handle(msg) {
  const { id, method, params } = msg;
  if (method === 'initialize') return { protocolVersion: params?.protocolVersion || '2025-06-18',
    capabilities: { tools: {} }, serverInfo: { name: 'alashi', version: '0.1.0' } };
  if (method === 'ping') return {};
  if (method === 'tools/list') return { tools: TOOLS.map(({ call, ...t }) => t) };
  if (method === 'tools/call') {
    const tool = TOOLS.find(t => t.name === params?.name);
    if (!tool) throw Object.assign(new Error('unknown tool'), { rpc: -32602 });
    let result;
    try { result = await tool.call(params.arguments || {}); } catch (error) { result = publicError(error); }
    return { content: [{ type: 'text', text: JSON.stringify(result, null, 2) }], isError: result?.ok === false };
  }
  if (id === undefined) return undefined; // notification
  throw Object.assign(new Error('method not found'), { rpc: -32601 });
}

const send = m => process.stdout.write(JSON.stringify(m) + '\n');
createInterface({ input: process.stdin }).on('line', async line => {
  if (!line.trim()) return;
  let msg;
  try { msg = JSON.parse(line); } catch { return send({ jsonrpc: '2.0', id: null, error: { code: -32700, message: 'parse error' } }); }
  try {
    const result = await handle(msg);
    if (msg.id !== undefined && result !== undefined) send({ jsonrpc: '2.0', id: msg.id, result });
  } catch (error) {
    if (msg.id !== undefined) send({ jsonrpc: '2.0', id: msg.id, error: { code: error.rpc || -32603, message: error.message } });
  }
});
