// Real owner-auth/API/runner path with a local deterministic provider only.
import assert from 'node:assert/strict';
import { createPrivateKey, createPublicKey, randomUUID } from 'node:crypto';
import { createServer } from 'node:http';
import { join } from 'node:path';
import { writeFileSync } from 'node:fs';
import { base58, ownerApi, modelProvider, runComparison } from './strategy-run.mjs';

const [base, directory] = process.argv.slice(2);
let requests = 0, active = 0, peak = 0;
const inputs = [];
const ad = {
  headline: 'Compare your approach with ALASHI',
  body: 'Use your agents on a shared brief. Review the result of each version.',
  cta: 'Explore ALASHI',
};
const model = createServer(async (req, res) => {
  let raw = '';
  for await (const chunk of req) raw += chunk;
  const body = JSON.parse(raw);
  inputs.push(body);
  requests++; active++; peak = Math.max(peak, active);
  await new Promise(resolve => setTimeout(resolve, 100));
  active--;
  res.writeHead(200, { 'Content-Type': 'application/json' });
  res.end(JSON.stringify({
    id: randomUUID(), usage: { prompt_tokens: 60, completion_tokens: 20, total_tokens: 80 },
    choices: [{ message: { content: JSON.stringify(ad) } }],
  }));
});
await new Promise(resolve => model.listen(0, '127.0.0.1', resolve));
try {
  const profiles = [1, 2].map((byte, i) => {
    const seed = Buffer.alloc(32, byte);
    const key = createPrivateKey({ key: Buffer.concat([Buffer.from('302e020100300506032b657004220420', 'hex'), seed]), format: 'der', type: 'pkcs8' });
    const pub = createPublicKey(key).export({ format: 'der', type: 'spki' }).subarray(-32);
    return { wallet: base58(pub), secret_key: [...seed, ...pub], agent_record_id: ['a', 'b'][i].repeat(64) };
  });
  const [owner, other] = await Promise.all(profiles.map(profile => ownerApi(base, profile)));
  const provider = modelProvider({ url: `http://127.0.0.1:${model.address().port}/chat/completions`, model: 'comparison-fixture', test: true });
  const beforeInstruction = 'Write an advertisement using the supplied facts.';
  const afterInstruction = beforeInstruction + ' Verify every required term before returning the final JSON.';
  const plan = instruction => [{ id: 'deliver', kind: 'model', depends_on: [], instruction, tool: null }];
  const baseline = (await owner.post('/strategies', { title: 'Private original', plan: plan(beforeInstruction), change: '', published: false })).version;
  const candidate = (await owner.post('/strategies', { parent: baseline.id, title: 'Private term check', plan: plan(afterInstruction), change: 'Check required terms in the delivery instruction.', published: false })).version;
  const challenge = (await owner.post('/strategy-challenges', {
    match_key: 'human-comparison-http', round_seconds: 180,
    title: 'Check an instruction on a shared brief', audience: 'Owners of AI agents',
    task: 'Write an English advertisement using only the supplied facts.',
    facts: ['Owners compare their approaches on a shared brief.', 'Each version has a separate result.'],
    required_terms: ['ALASHI', 'agents'], cta: ad.cta, max_words: 80, budget: 20000,
  })).challenge;
  const content = {
    challenge_id: challenge.id, baseline_version: baseline.id, candidate_version: candidate.id,
    hypothesis: 'An explicit term check will prevent missing required terms.',
    prediction: 'The candidate will include both required terms in its final ad.',
    decision_criterion: 'Keep the change only if the supplied evidence supports fewer omissions.',
    changed_task: 'deliver', attempt_limit: 2,
    profile: { source: provider.source, provider: provider.provider, model: provider.model, temperature: 0, max_tokens: 256 },
  };
  const comparison = (await owner.post('/comparisons', content)).comparison;
  assert.deepEqual(comparison.actual_diff, { task: 'deliver', field: 'instruction', before: beforeInstruction, after: afterInstruction });
  const list = api => api.request(`/agents/${api.record}/owner/comparisons`);
  assert.deepEqual((await list(other)).comparisons, [], 'another owner cannot read private comparisons');
  await assert.rejects(other.post(`/comparisons/${comparison.id}/attempts`, { client_id: 'unauthorized' }));
  const options = { api: owner, id: comparison.id, clientId: 'http-pair-01', provider, journalPrefix: join(directory, 'comparison-pair') };
  await assert.rejects(runComparison({ ...options, provider: { ...provider, model: 'different-model' } }), /comparison_provider_mismatch/);
  assert.equal((await list(owner)).comparisons[0].attempts.length, 0, 'profile rejection must precede attempt creation');
  let interrupted = false;
  const flaky = { ...owner, post: async (path, body) => {
    if (path.endsWith('/baseline/report') && !interrupted) {
      interrupted = true;
      throw new Error('simulated_report_connection_loss');
    }
    return owner.post(path, body);
  } };
  await assert.rejects(runComparison({ ...options, api: flaky }), /simulated_report_connection_loss/);
  assert.equal(requests, 2, 'both variants must finish their local provider call');
  await runComparison(options);
  assert.equal(requests, 2, 'resume must submit the saved answer without paying again');
  let saved = (await list(owner)).comparisons.find(value => value.id === comparison.id);
  assert.equal(saved.attempts.length, 1, 'same client ID must reuse the same pair');
  const attempt = saved.attempts[0];
  for (const run of [attempt.baseline, attempt.candidate]) {
    assert.equal(run.status, 'completed');
    assert.equal(run.result.checks.passed, true);
    assert.equal(run.known_tokens, 80);
    assert.equal(run.test_provider, true);
  }
  assert.equal(attempt.decision, null, 'format checks must not invent an owner decision');
  assert.ok(peak >= 2, 'independent variants should execute concurrently');
  assert.deepEqual(JSON.parse(inputs[0].messages[1].content).brief, JSON.parse(inputs[1].messages[1].content).brief);
  assert.ok(inputs.every(input => input.temperature === 0 && input.max_tokens === 256 && input.model === provider.model));
  const pub = await owner.request('/strategy-library');
  assert.ok(pub.runs.every(run => ![attempt.baseline.id, attempt.candidate.id].includes(run.id)), 'private runs must not enter the public duel collection');
  assert.ok(pub.versions.every(version => ![baseline.id, candidate.id].includes(version.id)), 'private instructions must not enter the public library');
  await owner.post(`/comparisons/${comparison.id}/attempts/${attempt.id}/decision`, {
    outcome: 'inconclusive', reason: 'Both deterministic fixtures meet the brief. This does not establish a benefit from the added instruction.',
  });
  saved = (await list(owner)).comparisons.find(value => value.id === comparison.id);
  assert.equal(saved.attempts[0].decision.outcome, 'inconclusive');
  const summary = {
    source: 'local_deterministic_fixture', real_provider_calls: 0, requests, peak,
    one_owner_two_private_versions: true, actual_instruction_diff: true,
    fixed_generation_settings: true, resumed_without_extra_calls: true,
    private_runs_absent_from_library: true, owner_decision: 'inconclusive',
    business_effect_measured: false, monetary_cost_measured: false,
  };
  writeFileSync(join(directory, 'comparison-http-result.json'), JSON.stringify(summary, null, 2));
  console.log(JSON.stringify(summary));
} finally {
  await new Promise(resolve => model.close(resolve));
}
