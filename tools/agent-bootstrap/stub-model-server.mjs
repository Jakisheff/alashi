#!/usr/bin/env node
// Stub of the python inference server for plumbing tests: answers with the
// first allowed action, shaped exactly like an OpenAI chat completion.
// On the GPU box this role belongs to the python server that serves the
// current ES-perturbed weights.
import { createServer } from 'node:http';

const PORT = Number(process.argv[2] ?? 7779);
const server = createServer(async (request, response) => {
  const chunks = [];
  for await (const chunk of request) chunks.push(chunk);
  let data; try { data = JSON.parse(Buffer.concat(chunks).toString('utf8')); } catch { response.writeHead(400); response.end('{}'); return; }
  const content = data?.messages?.[1]?.content;
  let action = null;
  try {
    const observation = typeof content === 'string' ? JSON.parse(content) : null;
    const menu = observation?.allowed_actions;
    if (Array.isArray(menu) && menu.length) action = menu[0];
  } catch {}
  const text = action === null ? 'pass' : JSON.stringify(action);
  response.writeHead(200, { 'Content-Type': 'application/json' });
  response.end(JSON.stringify({
    model: data?.model ?? 'stub',
    choices: [{ message: { role: 'assistant', content: text }, finish_reason: 'stop' }],
    usage: { prompt_tokens: 10, completion_tokens: 5, total_tokens: 15 },
  }));
});
server.listen(PORT, '127.0.0.1', () => console.error(`stub model server on :${PORT}`));
