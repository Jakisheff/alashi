// Called by the existing signed HTTP comparison fixture. No real provider calls.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { join } from 'node:path';
import { writeFileSync } from 'node:fs';
import { runComparison } from './strategy-run.mjs';

export async function exerciseResearch({ owner, other, content, provider, directory, base }) {
  const get = api => api.request(`/agents/${api.record}/owner/research`);
  const post = (api, action, body) => api.post(`/research/${action}`, body);
  const privateReason = 'PRIVATE-INDEPENDENT-REASON-sentinel';
  const privatePrediction = 'PRIVATE-OWNER-PREDICTION-sentinel';
  const comparison = (await owner.post('/comparisons', { ...content,
    hypothesis: 'Compare two instructions under a fixed criterion.',
    prediction: privatePrediction,
  })).comparison;
  const cid = comparison.id, states = [];
  const anonymous = await fetch(`${base}/agents/${owner.record}/owner/research`);
  assert.equal(anonymous.status, 401);
  await assert.rejects(other.request(`/agents/${owner.record}/owner/research`));
  let current = await post(owner, 'backlog-upsert', { client_id: 'research-http-backlog', title: 'Check an idea',
    hypothesis: 'PRIVATE-BACKLOG-sentinel', priority: 'later', comparison_id: cid });
  assert.equal(current.backlog.length, 1);
  current = await post(owner, 'backlog-upsert', { client_id: 'research-http-backlog', title: 'Check an idea',
    hypothesis: 'PRIVATE-BACKLOG-sentinel', priority: 'now', comparison_id: cid });
  assert.equal(current.backlog.length, 1);
  assert.equal(current.backlog[0].priority, 'now');
  assert.deepEqual((await get(other)).backlog, []);
  await assert.rejects(post(other, 'round-open', { comparison_id: cid, participants: [other.wallet] }));
  current = await post(owner, 'round-open', { comparison_id: cid, participants: [owner.wallet, other.wallet] });
  states.push(current);
  await assert.rejects(owner.post(`/comparisons/${cid}/attempts`, { client_id: 'blocked-unclosed-round' }), /forecast_round_open/);
  await assert.rejects(post(owner, 'round-close', { comparison_id: cid }), /forecast_required/);
  let peer = await get(other);
  assert.equal(peer.rounds.length, 1);
  assert.equal(peer.rounds[0].forecasts, null);
  assert.ok(!JSON.stringify(peer).includes(privatePrediction));
  assert.equal(peer.rounds[0].actual_diff, undefined);
  const forecast = { comparison_id: cid, probability_bps: 2500, reason: privateReason };
  peer = await post(other, 'forecast', forecast);
  states.push({wallet:other.wallet,response:peer});
  await post(other, 'forecast', forecast);
  await assert.rejects(post(other, 'forecast', { ...forecast, probability_bps: 7000 }), /forecast_frozen/);
  current = await get(owner);
  assert.equal(current.rounds[0].submitted_count, 1);
  assert.equal(current.rounds[0].forecasts, null);
  assert.ok(!JSON.stringify(current).includes(privateReason), 'owner cannot peek at a sealed peer forecast');
  current = await post(owner, 'forecast', { comparison_id: cid, probability_bps: 7500, reason: 'Owner reasoning before disclosure.' });
  assert.equal(current.rounds[0].forecasts, null, 'all submitted is still sealed until explicit closure');
  states.push(current);
  await assert.rejects(post(other, 'round-close', { comparison_id: cid }));
  current = await post(owner, 'round-close', { comparison_id: cid });
  const round = current.rounds[0];
  assert.equal(round.forecasts.length, 2);
  assert.equal(round.missing_count, 0);
  assert.equal(round.summary.mean_probability_bps, 5000);
  assert.equal(round.summary.min_probability_bps, 2500);
  assert.equal(round.summary.max_probability_bps, 7500);
  assert.ok(JSON.stringify(current).includes(privateReason));
  states.push(current);
  assert.equal((await get(other)).rounds[0].forecasts.length, 2);
  await post(other, 'forecast', forecast); // A lost reply can be retried after closure.
  const options = { api: owner, id: cid, clientId: 'research-http-attempt', provider,
    journalPrefix: join(directory, 'research-pair') };
  await runComparison(options);
  let saved = (await owner.request(`/agents/${owner.record}/owner/comparisons`)).comparisons.find(c => c.id === cid);
  const attempt = saved.attempts[0];
  assert.equal(attempt.candidate.status, 'completed');
  await assert.rejects(post(owner, 'apply', { comparison_id: cid, attempt_id: attempt.id }), /comparison_decision_required/);
  const publicFields = { title: 'Check required terms explicitly', mechanism: 'Add one explicit instruction to check required terms.',
    task: 'Write a short advertisement.', conditions: 'Same brief and generation profile, two variants.',
    result: 'Both fixture outputs met the format rules.', limitations: 'Synthetic provider. No business effect measured.',
    license: 'CC0-1.0', consent: true };
  await assert.rejects(post(owner, 'publish', { ...publicFields, comparison_id: cid, attempt_id: attempt.id }), /comparison_decision_required/);
  await owner.post(`/comparisons/${cid}/attempts/${attempt.id}/decision`, {
    outcome: 'adopt_candidate', reason: 'Select this version for the next task after reviewing both fixture outputs.',
  });
  current = await post(owner, 'apply', { comparison_id: cid, attempt_id: attempt.id });
  assert.equal(current.applied.version_id, content.candidate_version);
  const appliedAt = current.applied.applied_at;
  assert.equal((await post(owner, 'apply', { comparison_id: cid, attempt_id: attempt.id })).applied.applied_at, appliedAt);
  assert.equal((await get(other)).applied, null);
  states.push(current);
  assert.ok(current.digest.items.some(i => i.kind === 'decision_recorded'));
  assert.ok(current.digest.unread);
  current = await post(owner, 'digest-ack', { day: current.digest.day, fingerprint: current.digest.fingerprint });
  assert.equal(current.digest.unread, false);
  const publication = { ...publicFields, comparison_id: cid, attempt_id: attempt.id };
  await assert.rejects(post(owner, 'publish', { ...publication, consent: false }));
  await assert.rejects(post(owner, 'publish', { ...publication, raw_report: 'PRIVATE-REPORT-sentinel' }));
  current = await post(owner, 'publish', publication);
  states.push(current);
  await post(owner, 'publish', publication);
  await assert.rejects(post(owner, 'publish', { ...publication, result: 'Different after publication' }), /publication_frozen/);
  let pub = await owner.request('/research-library');
  assert.equal(pub.cards.length, 1);
  const card = pub.cards[0];
  assert.equal(card.evidence_status, 'model');
  for (const field of ['owner','record','source_id','attempt_id','comparison_id','forecasts','report','consent']) assert.equal(card[field], undefined);
  for (const sentinel of [privateReason, privatePrediction, 'PRIVATE-BACKLOG-sentinel', owner.wallet, cid, attempt.id]) {
    assert.ok(!JSON.stringify(pub).includes(sentinel), 'public card contains only explicitly reviewed fields');
  }
  peer = await post(other, 'library-reuse', { card_id: card.id, client_id: 'reuse-research-card' });
  assert.equal(peer.backlog[0].source_card_id, card.id);
  assert.equal(peer.backlog[0].priority, 'later');
  await post(other, 'library-reuse', { card_id: card.id, client_id: 'reuse-research-card' });
  assert.equal((await get(other)).backlog.length, 1);
  current = await post(owner, 'publish-local', { ...publicFields, client_id: 'local-research-export', title: 'Local-only example' });
  states.push(current);
  pub = await owner.request('/research-library');
  assert.equal(pub.cards.length, 2);
  assert.ok(pub.cards.some(c => c.evidence_status === 'owner_reported_local'));

  // Applying does not publish a private derivative or alter already-running work.
  const sharedBrief = (await other.post('/strategy-challenges', {
    match_key: 'research-selected-duel', round_seconds: 180, title: 'Next task', audience: 'Agent owners',
    task: 'Write an advertisement.', facts: ['ALASHI compares approaches.'], required_terms: ['ALASHI'],
    cta: 'Explore ALASHI', max_words: 80, budget: 20000,
  })).challenge;
  await assert.rejects(owner.post('/strategy-runs', { version: 'selected', challenge_id: sharedBrief.id,
    client_id: 'selected-run-fixture' }), /publish_derivative_first/);
  await owner.post(`/strategies/${content.baseline_version}/publish`, {});
  await owner.post(`/strategies/${content.candidate_version}/publish`, {});
  const started = (await owner.post('/strategy-runs', { version: 'selected', challenge_id: sharedBrief.id,
    client_id: 'selected-run-fixture' })).run;
  assert.equal(started.version, content.candidate_version);
  await runComparison({ ...options, clientId: 'research-second-attempt', journalPrefix: join(directory, 'research-second-pair') });
  saved = (await owner.request(`/agents/${owner.record}/owner/comparisons`)).comparisons.find(c => c.id === cid);
  const second = saved.attempts[1];
  await owner.post(`/comparisons/${cid}/attempts/${second.id}/decision`, { outcome: 'keep_baseline', reason: 'Choose the baseline for future work.' });
  await post(owner, 'apply', { comparison_id: cid, attempt_id: second.id });
  const retried = (await owner.post('/strategy-runs', { version: 'selected', challenge_id: sharedBrief.id,
    client_id: 'selected-run-fixture' })).run;
  assert.equal(retried.version, started.version, 'retry retains original selection after the default changes');
  assert.equal((await get(owner)).applied.version_id, content.baseline_version);
  assert.equal((await get(owner)).digest.unread, true, 'new same-day events make the digest unread again');
  const contract = execFileSync(process.execPath, ['--experimental-strip-types','--no-warnings',
    fileURLToPath(new URL('../../frontend/scripts/research-contract-check.ts', import.meta.url))], {
    input: JSON.stringify({ owner: owner.wallet, record: owner.record, states, public_response: pub }),
    encoding: 'utf8', timeout: 30_000,
  });
  assert.match(contract, /PASS/);
  const latest = await owner.get();
  writeFileSync(join(directory, 'research-ui-fixture.json'), JSON.stringify({
    owner: owner.wallet, record: owner.record, workspace: await get(owner),
    comparisons: (await owner.request(`/agents/${owner.record}/owner/comparisons`)).comparisons,
    versions: latest.versions, challenges: latest.challenges, library: pub,
  }, null, 2));
  const result = { independent_forecasts_sealed: true, owner_cannot_peek: true, cutoff_before_execution: true,
    disagreement_summary: true, daily_digest_ack: true, selected_version_used_by_next_run: true,
    idempotent_run_keeps_original_version: true, public_cards_allowlisted: true, local_claim_distinguished: true,
    public_card_reused_in_private_backlog: true, actual_http_responses_parse_in_frontend: true,
    provider: 'local_deterministic_fixture', business_effect_measured: false };
  writeFileSync(join(directory, 'research-http-result.json'), JSON.stringify(result, null, 2));
  return result;
}
