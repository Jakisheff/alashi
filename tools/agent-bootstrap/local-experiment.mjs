#!/usr/bin/env node
/** Private experiments: all coordination is local; only explicit model calls leave this process. */
import { createHash, randomBytes, randomUUID } from 'node:crypto';
import { existsSync, lstatSync, mkdirSync, openSync, closeSync, writeFileSync, fsyncSync, renameSync, unlinkSync, realpathSync, linkSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { isDeepStrictEqual } from 'node:util';
import { runCycle, modelProvider, readPrivate } from './strategy-run.mjs';

const ENGINE = 'alashi-local-ad-v1';
const labels = ['baseline', 'candidate'];
const fail = message => { throw new Error(message); };
const clone = value => structuredClone(value);
const canonical = value => JSON.stringify(value, function (_key, item) {
  return item && typeof item === 'object' && !Array.isArray(item)
    ? Object.fromEntries(Object.keys(item).sort().map(key => [key, item[key]])) : item;
});
const digest = value => createHash('sha256').update(canonical(value)).digest('hex');
const object = value => value && typeof value === 'object' && !Array.isArray(value);
const text = (value, max) => typeof value === 'string' && value.trim().length > 0 && Buffer.byteLength(value) <= max && !value.includes('\0');
const exactKeys = (value, keys) => object(value) && Object.keys(value).length === keys.length && keys.every(key => Object.hasOwn(value, key));
const validUsage = u => object(u) && ['prompt_tokens', 'completion_tokens', 'total_tokens', 'cached_tokens'].every(k => Number.isSafeInteger(u[k]) && u[k] >= 0)
  && u.prompt_tokens + u.completion_tokens === u.total_tokens && u.cached_tokens <= u.prompt_tokens;

function privateDirectory(dir) {
  const stat = lstatSync(dir);
  if (!stat.isDirectory() || stat.isSymbolicLink() || process.platform !== 'win32' && (stat.mode & 0o077)) fail('experiment_directory_requires_mode_700');
}
function read(path) {
  if (lstatSync(path).size > 2_000_000) fail('local_file_too_large');
  return readPrivate(path);
}
function save(path, value) {
  if (existsSync(path)) read(path); // Refuse symlinks and public permissions before replacing a file.
  const temporary = `${path}.tmp-${randomUUID()}`;
  const fd = openSync(temporary, 'wx', 0o600);
  try { writeFileSync(fd, JSON.stringify(value, null, 2) + '\n'); fsyncSync(fd); } finally { closeSync(fd); }
  try { renameSync(temporary, path); } catch (error) { unlinkSync(temporary); throw error; }
  const directory = openSync(dirname(path), 'r');
  try { fsyncSync(directory); } finally { closeSync(directory); }
}
function createExport(path, value) {
  const temporary = `${path}.tmp-${randomUUID()}`, fd = openSync(temporary, 'wx', 0o600);
  try { writeFileSync(fd, JSON.stringify(value, null, 2) + '\n'); fsyncSync(fd); } finally { closeSync(fd); }
  try {
    // Publish a complete file without replacing a destination created by another process.
    try { linkSync(temporary, path); } catch (error) { if (error.code === 'EEXIST') fail('public_export_output_exists'); throw error; }
    const directory = openSync(dirname(path), 'r');
    try { fsyncSync(directory); } finally { closeSync(directory); }
  } finally { unlinkSync(temporary); }
}
async function locked(dir, work) {
  privateDirectory(dir);
  const lock = join(dir, '.experiment.lock');
  let fd;
  try { fd = openSync(lock, 'wx', 0o600); } catch (error) { if (error.code === 'EEXIST') fail('experiment_busy_check_existing_process'); throw error; }
  try { writeFileSync(fd, JSON.stringify({ pid: process.pid })); fsyncSync(fd); } finally { closeSync(fd); }
  try { return await work(); } finally { unlinkSync(lock); }
}

function validatePlan(plan) {
  if (!Array.isArray(plan) || !plan.length || plan.length > 8 || plan.at(-1)?.id !== 'deliver' || plan.at(-1)?.kind !== 'model') fail('invalid_local_plan');
  const seen = new Set();
  for (const task of plan) {
    if (!exactKeys(task, ['id', 'kind', 'depends_on', 'instruction', 'tool']) || typeof task.id !== 'string' || !/^[a-z0-9_-]{1,32}$/.test(task.id) || seen.has(task.id)
        || !text(task.instruction, 2000) || !Array.isArray(task.depends_on) || new Set(task.depends_on).size !== task.depends_on.length
        || !task.depends_on.every(id => seen.has(id)) || !['model', 'tool'].includes(task.kind)
        || (task.kind === 'model' ? task.tool !== null : !['brief_extract', 'copy_check'].includes(task.tool))) fail('invalid_local_plan');
    seen.add(task.id);
  }
  const needed = new Set(['deliver']);
  for (const task of [...plan].reverse()) if (needed.has(task.id)) task.depends_on.forEach(id => needed.add(id));
  if (needed.size !== plan.length) fail('unused_local_task');
}
export function validateManifest(manifest) {
  if (!exactKeys(manifest, ['engine', 'hypothesis', 'prediction', 'decision_criterion', 'failure_conditions', 'brief', 'baseline', 'candidate', 'profile', 'budget', 'duration_seconds']) || manifest.engine !== ENGINE) fail('invalid_local_manifest');
  for (const field of ['hypothesis', 'prediction', 'decision_criterion', 'failure_conditions']) if (!text(manifest[field], 3000)) fail(`invalid_${field}`);
  const brief = manifest.brief;
  if (!exactKeys(brief, ['title', 'audience', 'task', 'facts', 'required_terms', 'cta', 'max_words']) || !text(brief.title, 100)
      || !text(brief.audience, 500) || !text(brief.task, 2000) || !Array.isArray(brief.facts) || brief.facts.length < 1 || brief.facts.length > 12
      || !brief.facts.every(f => text(f, 500)) || !Array.isArray(brief.required_terms) || brief.required_terms.length < 1 || brief.required_terms.length > 8
      || !brief.required_terms.every(t => text(t, 80) && /^[\x00-\x7f]+$/.test(t)) || !text(brief.cta, 120) || !/^[\x00-\x7f]+$/.test(brief.cta)
      || !Number.isInteger(brief.max_words) || brief.max_words < 20 || brief.max_words > 200) fail('invalid_local_brief');
  labels.forEach(label => validatePlan(manifest[label]));
  let changed = 0;
  if (manifest.baseline.length !== manifest.candidate.length) fail('one_instruction_change_required');
  manifest.baseline.forEach((task, i) => {
    const candidate = manifest.candidate[i];
    if (!isDeepStrictEqual({ ...task, instruction: '' }, { ...candidate, instruction: '' })) fail('one_instruction_change_required');
    if (task.instruction !== candidate.instruction) { if (task.kind !== 'model') fail('one_instruction_change_required'); changed++; }
  });
  if (changed !== 1) fail('one_instruction_change_required');
  const profile = manifest.profile;
  if (!exactKeys(profile, ['url', 'model', 'source', 'temperature', 'max_tokens']) || !['provider_reported', 'test_provider'].includes(profile.source)
      || !text(profile.model, 128) || typeof profile.url !== 'string' || profile.temperature !== 0 || profile.max_tokens !== 256) fail('invalid_local_profile');
  modelProvider({ url: profile.url, model: profile.model, test: profile.source === 'test_provider' });
  const providerHost = new URL(profile.url).hostname;
  if (providerHost === 'alashi.network' || providerHost.endsWith('.alashi.network')) fail('alashi_is_not_a_private_model_endpoint');
  if (!Number.isSafeInteger(manifest.budget) || manifest.budget < 1 || manifest.budget > 1_000_000
      || !Number.isInteger(manifest.duration_seconds) || manifest.duration_seconds < 60 || manifest.duration_seconds > 900) fail('invalid_local_budget_or_duration');
  return manifest;
}

export function exampleManifest() {
  const baseline = [
    { id: 'facts', kind: 'tool', depends_on: [], instruction: 'Extract the supplied brief facts.', tool: 'brief_extract' },
    { id: 'deliver', kind: 'model', depends_on: ['facts'], instruction: 'Write a clear advertisement using the supplied facts.', tool: null },
  ];
  const candidate = clone(baseline);
  candidate[1].instruction = 'Write a clear advertisement. Put the concrete source fact first in the headline.';
  return { engine: ENGINE, hypothesis: 'Putting a concrete source fact first makes the draft easier to review.',
    prediction: 'The candidate headline will state a supplied source fact.', decision_criterion: 'Use the candidate only if I prefer its draft after checking both against the brief.',
    failure_conditions: 'Unknown usage, missing evidence or an invented claim makes the result inconclusive.',
    brief: { title: 'Example private brief', audience: 'Teams comparing product ideas', task: 'Write one advertisement from these facts.',
      facts: ['Acme offers a weekly workshop for teams.'], required_terms: ['Acme', 'workshop'], cta: 'Explore Acme', max_words: 80 },
    baseline, candidate, profile: { url: 'https://provider.example/v1/chat/completions', model: 'REPLACE_WITH_YOUR_MODEL', source: 'provider_reported', temperature: 0, max_tokens: 256 },
    budget: 30000, duration_seconds: 300 };
}
export function initExperiment(dir, baselinePath) {
  mkdirSync(dir, { mode: 0o700 });
  const manifest = exampleManifest();
  if (baselinePath) {
    const selected = read(resolve(baselinePath)); validatePlan(selected.plan);
    manifest.baseline = clone(selected.plan); manifest.candidate = clone(selected.plan);
    manifest.candidate.at(-1).instruction += '\nPut the concrete source fact first in the headline.';
  }
  save(join(dir, 'manifest.json'), manifest);
  return { directory: dir, next: 'Edit manifest.json, then freeze. No network requests were made.' };
}
export async function freezeExperiment(dir) {
  return locked(dir, async () => {
    const manifest = validateManifest(read(join(dir, 'manifest.json')));
    const path = join(dir, 'frozen.json');
    if (existsSync(path)) return loadFrozen(dir);
    const salt = randomBytes(32).toString('hex'), frozen = { manifest, salt, digest: digest({ manifest, salt }) };
    save(path, frozen); return frozen;
  });
}
function loadFrozen(dir) {
  const frozen = read(join(dir, 'frozen.json'));
  validateManifest(frozen.manifest);
  if (!/^[0-9a-f]{64}$/.test(frozen.salt) || frozen.digest !== digest({ manifest: frozen.manifest, salt: frozen.salt })
      || !isDeepStrictEqual(frozen.manifest, read(join(dir, 'manifest.json')))) fail('frozen_manifest_changed_create_new_experiment');
  return frozen;
}

export function artifactChecks(brief, artifact) {
  const schema = exactKeys(artifact, ['headline', 'body', 'cta']) && ['headline', 'body', 'cta'].every(k => typeof artifact[k] === 'string');
  const parts = ['headline', 'body', 'cta'].map(k => typeof artifact?.[k] === 'string' ? artifact[k] : ''), joined = parts.join(' '), lower = joined.toLowerCase();
  const words = joined.trim().split(/\s+/).filter(Boolean).length;
  const checks = { schema, english_ascii: schema && /^[\x00-\x7f]*$/.test(joined) && parts.every(p => p.trim()),
    length: words >= 10 && words <= brief.max_words && parts[0].trim().split(/\s+/).length <= 12,
    required_terms: brief.required_terms.every(term => lower.includes(term.toLowerCase())), cta: parts[2].trim() === brief.cta,
    no_unsubstantiated_numeric_claims: !/[%$]/.test(joined) && !['guaranteed', 'always wins', 'risk-free', 'zero cost'].some(term => lower.includes(term)) };
  return { ...checks, english_ascii: !!checks.english_ascii, word_count: words, passed: Object.values(checks).every(Boolean),
    limit: 'format_checks_do_not_establish_creative_quality_or_ad_performance' };
}
function newState(frozen, now) {
  const { manifest, digest: id } = frozen;
  const versions = labels.map(label => ({ id: digest({ id, label, type: 'version' }), content: { plan: manifest[label] } }));
  return { digest: id, engine: ENGINE, versions, runs: labels.map((label, i) => ({ id: digest({ id, label, type: 'run' }), record: id,
    label, owner: 'local-owner', version: versions[i].id, status: 'running', deadline_at: Math.floor(now / 1000) + manifest.duration_seconds,
    calls: {}, brief: manifest.brief, known_tokens: 0, budget: manifest.budget })), decision: null, public_export: null };
}
function readState(dir, frozen) {
  const state = read(join(dir, 'state.json'));
  const expected = newState(frozen, 0);
  if (state.digest !== frozen.digest || state.engine !== ENGINE || !isDeepStrictEqual(state.versions, expected.versions) || !Array.isArray(state.runs) || state.runs.length !== 2
      || state.runs.some((r, i) => !object(r.calls) || !['id', 'record', 'label', 'owner', 'version', 'brief', 'budget'].every(k => isDeepStrictEqual(r[k], expected.runs[i][k])))) fail('local_state_manifest_mismatch');
  for (const [index, run] of state.runs.entries()) {
    const plan = frozen.manifest[labels[index]];
    if (!Number.isSafeInteger(run.deadline_at) || run.deadline_at < 1 || run.deadline_at !== state.runs[0].deadline_at
        || !['running', 'completed', 'stopped', 'usage_unknown', 'quality_failed', 'budget_overrun'].includes(run.status)) fail('invalid_local_run_state');
    for (const [task, call] of Object.entries(run.calls)) {
      const spec = plan.find(spec => spec.id === task);
      if (!spec || !object(call) || !Number.isSafeInteger(call.reserved) || call.reserved < 0 || call.reserved > run.budget
          || !/^[0-9a-f]{64}$/.test(call.execution_id) || call.execution_claimed !== true
          || !['reserved', 'reported', 'failed'].includes(call.status) || call.usage !== null && !validUsage(call.usage)
          || ![null, true, false].includes(call.quality_passed)) fail('invalid_local_call_state');
      if (call.status === 'reserved' && (call.usage !== null || call.quality_passed !== null || call.receipt)
          || call.status !== 'reserved' && (!object(call.receipt) || !isDeepStrictEqual(call.usage, call.receipt.usage) || !isDeepStrictEqual(call.report, call.receipt.report))) fail('invalid_local_receipt_state');
      const identity = spec.kind === 'tool' ? { source: 'local_tool', provider: 'alashi-local-tools', model: 'none' }
        : { source: frozen.manifest.profile.source, provider: new URL(frozen.manifest.profile.url).origin, model: frozen.manifest.profile.model };
      if (!Object.keys(identity).every(key => call[key] === identity[key]) || spec.kind === 'tool' && (call.reserved !== 0 || call.usage && call.usage.total_tokens !== 0)) fail('invalid_local_provider_state');
      if (call.receipt && (!exactKeys(call.receipt, ['usage', 'report', 'response_id', 'request_failed']) || typeof call.receipt.request_failed !== 'boolean'
          || call.quality_passed !== (call.receipt.request_failed === false && !!object(call.report) && Object.keys(call.report).length > 0
          && (task !== 'deliver' || artifactChecks(run.brief, call.report).passed)))) fail('invalid_local_receipt_state');
      if (call.quality_passed === true && (!object(call.report) || !Object.keys(call.report).length || task === 'deliver' && !artifactChecks(run.brief, call.report).passed)) fail('invalid_local_quality_state');
    }
    if (!Number.isSafeInteger(run.known_tokens) || run.known_tokens !== Object.values(run.calls).reduce((total, call) => total + (call.usage?.total_tokens ?? 0), 0)) fail('invalid_local_usage_total');
    if (run.status === 'completed' && (!plan.every(task => run.calls[task.id]?.quality_passed && run.calls[task.id]?.usage)
        || !isDeepStrictEqual(run.result?.artifact, run.calls.deliver.report)
        || !isDeepStrictEqual(run.result?.checks, artifactChecks(run.brief, run.calls.deliver.report)))) fail('invalid_completed_local_run');
  }
  if (state.decision !== null && (!exactKeys(state.decision, ['choice', 'reason']) || !['keep_baseline', 'adopt_candidate', 'inconclusive'].includes(state.decision.choice)
      || !text(state.decision.reason, 3000) || state.runs.some(run => Object.values(run.calls).some(call => call.status === 'reserved'))
      || state.decision.choice !== 'inconclusive' && state.runs.some(run => run.status !== 'completed'))) fail('invalid_local_decision_state');
  if (state.public_export !== null && (!exactKeys(state.public_export, ['client_id', 'card_digest'])
      || !/^[0-9a-f-]{36}$/.test(state.public_export.client_id) || !/^[0-9a-f]{64}$/.test(state.public_export.card_digest))) fail('invalid_local_export_state');
  return state;
}
function adapter(state, persist) {
  const snapshot = () => clone({ versions: state.versions, runs: state.runs });
  return { record: state.digest, get: async () => snapshot(), request: async path => {
    if (path !== '/strategy-library') fail('network_route_not_supported'); return { versions: [] };
  }, post: async (path, body) => {
    const match = path.match(/^\/strategy-runs\/([0-9a-f]{64})\/(reserve|report|finish)$/);
    const run = match && state.runs.find(r => r.id === match[1]);
    if (!run) fail('local_run_not_found');
    const plan = state.versions.find(v => v.id === run.version).content.plan;
    if (match[2] === 'finish') {
      if (run.status !== 'running' || !plan.every(t => run.calls[t.id]?.quality_passed && run.calls[t.id]?.usage)) fail('verification_required');
      run.status = 'completed'; run.result = { artifact: run.calls.deliver.report, checks: artifactChecks(run.brief, run.calls.deliver.report) };
    } else {
      const spec = plan.find(t => t.id === body.task); if (!spec) fail('unknown_local_task');
      const old = run.calls[body.task];
      if (match[2] === 'reserve') {
        if (old) {
          if (old.execution_id !== body.execution_id || old.reserved !== body.max_total_tokens
              || !['source', 'provider', 'model'].every(k => old[k] === body[k])) fail('execution_conflict');
          return { run: clone(run) };
        }
        if (run.status !== 'running') fail('run_stopped');
        if (!spec.depends_on.every(id => run.calls[id]?.quality_passed && run.calls[id]?.usage)) fail('dependencies_not_complete');
        const held = Object.values(run.calls).reduce((total, call) => total + (call.usage ? 0 : call.reserved), 0);
        if (!Number.isSafeInteger(body.max_total_tokens) || body.max_total_tokens < 0 || run.known_tokens + held + body.max_total_tokens > run.budget) fail('budget_exhausted');
        run.calls[body.task] = { reserved: body.max_total_tokens, status: 'reserved', usage: null, quality_passed: null,
          source: body.source, provider: body.provider, model: body.model, execution_id: body.execution_id, execution_claimed: true };
      } else {
        if (!old || old.execution_id !== body.execution_id) fail('execution_conflict');
        const usage = validUsage(body.usage) ? body.usage : null;
        const quality = body.request_failed === false && !!object(body.report) && Object.keys(body.report).length > 0
          && (body.task !== 'deliver' || artifactChecks(run.brief, body.report).passed);
        const receipt = { usage, report: body.report, response_id: body.response_id, request_failed: body.request_failed };
        if (old.receipt && !isDeepStrictEqual(old.receipt, receipt)) fail('idempotency_conflict');
        Object.assign(old, { usage, report: body.report, receipt, quality_passed: quality, status: body.request_failed ? 'failed' : 'reported' });
        run.known_tokens = Object.values(run.calls).reduce((total, call) => total + (call.usage?.total_tokens ?? 0), 0);
        run.status = Object.values(run.calls).some(c => c.usage?.total_tokens > c.reserved) || run.known_tokens > run.budget ? 'budget_overrun'
          : Object.values(run.calls).some(c => c.status !== 'reserved' && !c.usage) ? 'usage_unknown'
          : Object.values(run.calls).some(c => c.quality_passed === false) ? 'quality_failed' : 'running';
      }
    }
    persist(); return { run: clone(run) };
  } };
}
export async function runExperiment({ dir, allowProvider, key, provider, now = Date.now(), onProgress = () => {} }) {
  if (allowProvider !== true) fail('provider_receives_private_brief_and_prompts_require_allow_provider_yes');
  return locked(dir, async () => {
    const frozen = loadFrozen(dir), profile = frozen.manifest.profile;
    const actual = provider ?? modelProvider({ url: profile.url, key, model: profile.model, test: profile.source === 'test_provider' });
    if (actual.source !== profile.source || actual.provider !== new URL(profile.url).origin || actual.model !== profile.model
        || actual.temperature !== profile.temperature || actual.max_tokens !== profile.max_tokens) fail('local_provider_mismatch');
    const path = join(dir, 'state.json'), state = existsSync(path) ? readState(dir, frozen) : newState(frozen, now);
    if (state.decision) fail('decision_already_recorded');
    const persist = () => save(path, state); persist();
    const api = adapter(state, persist);
    const results = await Promise.allSettled(state.runs.map(run => runCycle({ api, id: run.id, provider: actual,
      journalPath: join(dir, `${run.label}.journal.json`), onProgress: event => onProgress({ label: run.label, ...event }) })));
    results.forEach((result, i) => {
      if (result.status === 'rejected') {
        state.runs[i].error = /^[a-z0-9_]{1,100}$/.test(result.reason.message) ? result.reason.message : 'local_execution_failed';
        if (state.runs[i].status === 'running') state.runs[i].status = 'stopped';
      }
    });
    persist(); return summary(state);
  });
}
function summary(state) {
  return { digest: state.digest, status: state.runs.every(r => r.status === 'completed') ? 'ready_for_owner_decision' : 'inconclusive',
    runs: state.runs.map(r => ({ label: r.label, status: r.status, known_tokens: r.known_tokens, error: r.error ?? null })),
    decision: state.decision, alashi_requests: 0, provider_usage: 'owner_submitted_receipts', cost_usd: null };
}
export async function decideExperiment(dir, choice, reason) {
  return locked(dir, async () => {
    const frozen = loadFrozen(dir), state = readState(dir, frozen);
    if (!['keep_baseline', 'adopt_candidate', 'inconclusive'].includes(choice) || !text(reason, 3000)) fail('invalid_local_decision');
    const decision = { choice, reason };
    if (state.decision) { if (!isDeepStrictEqual(state.decision, decision)) fail('decision_immutable'); return summary(state); }
    if (state.runs.some(r => Object.values(r.calls).some(c => c.status === 'reserved'))) fail('resume_to_settle_reserved_calls');
    if (state.runs.some(r => r.status !== 'completed') && choice !== 'inconclusive') fail('incomplete_evidence_requires_inconclusive');
    state.decision = decision; save(join(dir, 'state.json'), state); return summary(state);
  });
}
export async function applyExperiment(dir) {
  return locked(dir, async () => {
    const frozen = loadFrozen(dir), state = readState(dir, frozen), choice = state.decision?.choice;
    if (!['keep_baseline', 'adopt_candidate'].includes(choice)) fail('completed_owner_choice_required');
    const label = choice === 'adopt_candidate' ? 'candidate' : 'baseline';
    const selected = { engine: ENGINE, experiment: frozen.digest, label, plan: frozen.manifest[label] };
    const path = join(dir, 'active-strategy.json'); save(path, selected);
    return { active_strategy: path, label, next: 'Use init --baseline with this file for the next local experiment.' };
  });
}
export async function exportExperiment(dir, cardPath, outputPath, consent) {
  if (consent !== true) fail('public_export_requires_explicit_consent');
  return locked(dir, async () => {
    const frozen = loadFrozen(dir), state = readState(dir, frozen);
    if (!state.decision) fail('owner_decision_required');
    const card = read(resolve(cardPath)), fields = ['title', 'mechanism', 'task', 'conditions', 'result', 'limitations'];
    const limits = { title: 100, mechanism: 2000, task: 2000, conditions: 2000, result: 4000, limitations: 4000 };
    if (!exactKeys(card, fields) || !fields.every(field => text(card[field], limits[field])) || /\p{Cc}/u.test(card.title)) fail('invalid_public_card_fields');
    const output = resolve(outputPath);
    const canonicalOutput = join(realpathSync(dirname(output)), output.split('/').at(-1));
    if (canonicalOutput === realpathSync(resolve(cardPath))) fail('public_export_requires_separate_output');
    if (dirname(canonicalOutput) === realpathSync(dir) && ['manifest.json', 'frozen.json', 'state.json', 'active-strategy.json', 'baseline.journal.json', 'candidate.journal.json', '.experiment.lock', 'baseline.journal.json.lock', 'candidate.journal.json.lock'].includes(canonicalOutput.split('/').at(-1))) fail('export_path_conflicts_with_private_state');
    const cardDigest = digest(card);
    if (state.public_export && state.public_export.card_digest !== cardDigest) fail('export_intent_immutable_create_new_experiment');
    const intent = state.public_export ?? { client_id: randomUUID(), card_digest: cardDigest };
    const exported = { client_id: intent.client_id, ...card, consent: true, license: 'CC0-1.0' };
    const existing = existsSync(output);
    if (existing && !isDeepStrictEqual(read(output), exported)) fail('public_export_output_exists');
    if (!state.public_export) { state.public_export = intent; save(join(dir, 'state.json'), state); }
    if (!existing) createExport(output, exported);
    return { export_file: output, client_id: exported.client_id, published: false, next: 'Review this file before importing it into the Public Goods library.' };
  });
}

const help = `Local private experiment (Node 20+). No ALASHI account or ALASHI HTTP calls.
  node local-experiment.mjs init --dir PRIVATE_DIRECTORY [--baseline ACTIVE_STRATEGY_FILE]
  Edit manifest.json: question, failure conditions, brief, plans and provider profile.
  node local-experiment.mjs freeze --dir PRIVATE_DIRECTORY
  ALASHI_MODEL_KEY=... node local-experiment.mjs run --dir PRIVATE_DIRECTORY --allow-provider yes
  node local-experiment.mjs report --dir PRIVATE_DIRECTORY
  node local-experiment.mjs decide --dir PRIVATE_DIRECTORY --choice adopt_candidate --reason "Your observed reason"
  node local-experiment.mjs apply --dir PRIVATE_DIRECTORY
  node local-experiment.mjs export --dir PRIVATE_DIRECTORY --card REDACTED_CARD_JSON --out PUBLIC_CARD_JSON --consent yes
Choices: keep_baseline, adopt_candidate, inconclusive. Card fields: title, mechanism, task, conditions, result, limitations.
The configured provider receives the private brief, instructions and dependency reports. Local files are not encrypted.
Set a loopback endpoint and source=test_provider for offline mock tests. No automatic retry of ambiguous provider calls.
Export only copies your separately authored card. It grants CC0-1.0 reuse; it does not upload or publish anything.
This v1 scenario checks static English ad format. It does not measure sales, model truth, or onchain execution.`;
export async function main(args = process.argv.slice(2)) {
  if (!args.length || ['--help', 'help'].includes(args[0])) { console.log(help); return; }
  const [command, ...flags] = args, opts = {};
  for (let i = 0; i < flags.length; i += 2) {
    if (!/^--[a-z-]+$/.test(flags[i]) || !flags[i + 1] || flags[i + 1].startsWith('--') || Object.hasOwn(opts, flags[i].slice(2))) fail('expected_unique_flag_value_pairs');
    opts[flags[i].slice(2)] = flags[i + 1];
  }
  const allowed = { init: ['dir', 'baseline'], freeze: ['dir'], run: ['dir', 'allow-provider'], report: ['dir'],
    decide: ['dir', 'choice', 'reason'], apply: ['dir'], export: ['dir', 'card', 'out', 'consent'] };
  if (!allowed[command] || Object.keys(opts).some(k => !allowed[command].includes(k)) || !opts.dir) fail('invalid_command_use_help');
  const dir = resolve(opts.dir); let result;
  if (command === 'init') result = initExperiment(dir, opts.baseline);
  if (command === 'freeze') { const frozen = await freezeExperiment(dir); result = { digest: frozen.digest, status: 'frozen' }; }
  if (command === 'run') result = await runExperiment({ dir, allowProvider: opts['allow-provider'] === 'yes', key: process.env.ALASHI_MODEL_KEY });
  if (command === 'report') { privateDirectory(dir); result = readState(dir, loadFrozen(dir)); }
  if (command === 'decide') result = await decideExperiment(dir, opts.choice, opts.reason);
  if (command === 'apply') result = await applyExperiment(dir);
  if (command === 'export') { if (!opts.card || !opts.out) fail('card_and_out_required'); result = await exportExperiment(dir, opts.card, opts.out, opts.consent === 'yes'); }
  console.log(JSON.stringify(result, null, 2));
}
if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) main().catch(error => {
  console.error(/^[a-z0-9_]{1,100}$/.test(error.message) ? error.message : 'local_experiment_failed'); process.exitCode = 1;
});
