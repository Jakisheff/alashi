import { useEffect, useRef, useState } from 'react'
import { LiveApiError } from '../live/api/client'
import { comparisonDecisionOptions, comparisons, errorText, instructionDiff, projectComparison, strategyRequest, type Challenge, type Comparison, type ComparisonDecision, type Run, type Version } from './client'

const short = (id: string) => id.slice(0, 8)
const outcomes: Record<ComparisonDecision['outcome'], string> = { keep_baseline: 'Keep baseline', adopt_candidate: 'Adopt candidate', inconclusive: 'Inconclusive' }

function ComparisonResult({ label, run }: { label: string; run: Run }) {
  return <article className="comparison-result"><span className="eyebrow">{label}</span><h4>{run.status.replaceAll('_', ' ')}</h4>
    <p><strong>{run.known_tokens.toLocaleString('en-US')}</strong> {run.test_provider ? 'TEST tokens' : 'reported model tokens'} · {Object.keys(run.calls).length} tasks</p>
    <p>Deadline: {run.deadline_at ? new Date(run.deadline_at * 1000).toLocaleString() : 'not available'}.</p>
    <p>Monetary cost: unknown.{run.unknown_usage && ' Token usage is incomplete.'}</p>
    {run.result ? <><div className="submitted-ad"><h4>{run.result.artifact.headline}</h4><p>{run.result.artifact.body}</p><strong>{run.result.artifact.cta}</strong></div><p>{run.result.checks.passed ? 'Format checks passed' : 'Format checks failed'} · {run.result.checks.word_count} words</p></> : <p>No completed advertisement yet.</p>}
    <details><summary>Task receipts</summary><ul className="comparison-receipts">{Object.values(run.calls).map(call => <li key={call.task}><b>{call.task}</b>: {call.status} · {call.source === 'local_tool' ? 'local tool' : call.model} · {call.usage ? `${call.usage.total_tokens} tokens` : 'usage unknown'}</li>)}</ul></details>
  </article>
}

export function ComparisonPanel({ record, owner, versions, challenges, onEdit, onExpired }: { record: string; owner: string; versions: Version[]; challenges: Challenge[]; onEdit: (version: Version) => void; onExpired: () => void }) {
  const [items, setItems] = useState<Comparison[]>([]), [selected, setSelected] = useState(''), [error, setError] = useState(''), [busy, setBusy] = useState(false), [notice, setNotice] = useState('')
  const [baselineId, setBaselineId] = useState(''), [candidateId, setCandidateId] = useState(''), [challengeId, setChallengeId] = useState('')
  const [hypothesis, setHypothesis] = useState(''), [prediction, setPrediction] = useState(''), [criterion, setCriterion] = useState(''), [provider, setProvider] = useState(''), [model, setModel] = useState(''), [test, setTest] = useState(false)
  const [outcome, setOutcome] = useState<ComparisonDecision['outcome']>('inconclusive'), [reason, setReason] = useState(''), [now, setNow] = useState(0), [retryComparison, setRetryComparison] = useState('')
  const active = useRef(true), guard = useRef(false), pendingAttempt = useRef<{ comparison: string; client: string } | null>(null)
  const prefix = `/agents/${record}/owner/comparisons`, own = versions.filter(v => v.content.owner === owner), briefs = challenges.filter(c => !c.content.trading)
  const baseline = own.find(v => v.id === baselineId), candidate = own.find(v => v.id === candidateId), diff = baseline && candidate ? instructionDiff(baseline, candidate) : null
  const current = items.find(c => c.id === selected), attempt = current?.attempts.at(-1), decisionOptions = attempt ? comparisonDecisionOptions(attempt, now) : []
  const chosenOutcome = decisionOptions.includes(outcome) ? outcome : 'inconclusive'

  useEffect(() => {
    active.current = true; const controller = new AbortController(); let timer = 0
    async function poll() {
      try { const next = comparisons(await strategyRequest(prefix, undefined, controller.signal), owner, record); if (!controller.signal.aborted && !guard.current) { setItems(next); setNow(Math.floor(Date.now() / 1000)); setSelected(id => id || next[0]?.id || '') } }
      catch (e) { if (!controller.signal.aborted) { setError(errorText(e)); if (e instanceof LiveApiError && ['owner_session_invalid', 'owner_session_expired'].includes(e.code)) { setItems([]); onExpired() } } }
      if (!controller.signal.aborted) timer = window.setTimeout(poll, 1500)
    }
    void poll(); return () => { active.current = false; controller.abort(); clearTimeout(timer) }
  }, [prefix, owner, record, onExpired])

  function retain(next: Comparison) { if (active.current) { setItems(old => [next, ...old.filter(c => c.id !== next.id)]); setSelected(next.id) } }
  async function perform(fn: () => Promise<void>) {
    if (guard.current) return
    guard.current = true; setBusy(true); setError(''); setNotice('')
    try { await fn() } catch (e) { if (active.current) { setError(errorText(e)); if (e instanceof LiveApiError && ['owner_session_invalid', 'owner_session_expired'].includes(e.code)) { setItems([]); onExpired() } } }
    finally { guard.current = false; if (active.current) setBusy(false) }
  }

  return <section className="comparison-panel" aria-labelledby="comparison-title"><div className="comparison-heading"><div><span className="eyebrow">PRIVATE OWNER EXPERIMENT</span><h3 id="comparison-title">Test your hypothesis with your agents</h3><p>Compare your baseline with one instruction change. Write your prediction and decision rule before either version runs.</p></div><span className="tag">Only visible to you</span></div>
    <form onSubmit={e => { e.preventDefault(); if (!diff) return; void perform(async () => {
      const reply = await strategyRequest(prefix, { challenge_id: challengeId, baseline_version: baselineId, candidate_version: candidateId, hypothesis: hypothesis.trim(), prediction: prediction.trim(), decision_criterion: criterion.trim(), changed_task: diff.task, profile: { source: test ? 'test_provider' : 'provider_reported', provider: new URL(provider.trim()).origin, model: model.trim(), temperature: 0, max_tokens: 256 }, attempt_limit: 1 })
      retain(projectComparison(reply.comparison, owner, record)); if (active.current) setNotice('Comparison frozen. No model has been called.')
    }) }}>
      <div className="comparison-columns"><label>Your hypothesis<textarea required maxLength={2000} value={hypothesis} placeholder="I think a more specific audience instruction will make the ad clearer." onChange={e => setHypothesis(e.target.value)} /></label><div><label>Your prediction<textarea required maxLength={2000} value={prediction} placeholder="What observable difference do you expect?" onChange={e => setPrediction(e.target.value)} /></label><label>Your decision rule<textarea required maxLength={2000} value={criterion} placeholder="What result would make you keep or reject this change?" onChange={e => setCriterion(e.target.value)} /></label></div></div>
      <label>Shared advertisement brief<select required value={challengeId} onChange={e => setChallengeId(e.target.value)}><option value="">Select an existing advertisement brief</option>{briefs.map(c => <option key={c.id} value={c.id}>{c.content.title} · {short(c.id)}</option>)}</select></label>
      {!briefs.length && <p>Create an advertisement brief in the existing host controls below, then return here. This experiment does not take a duel seat or start its clock.</p>}
      {briefs.find(c => c.id === challengeId) && <details><summary>Read the frozen brief</summary><p>{briefs.find(c => c.id === challengeId)!.content.task}</p><ul>{briefs.find(c => c.id === challengeId)!.content.facts.map(f => <li key={f}>{f}</li>)}</ul></details>}
      <div className="comparison-columns">{[['Baseline', baselineId, setBaselineId], ['Candidate', candidateId, setCandidateId]].map(([label, value, setter]) => <label key={label as string}>{label as string}<select required value={value as string} onChange={e => (setter as (id: string) => void)(e.target.value)}><option value="">Select your saved version</option>{own.map(v => <option key={v.id} value={v.id}>{v.content.title} · {v.published ? 'public' : 'private'} · {short(v.id)}</option>)}</select></label>)}</div>
      <p>Private versions are allowed. To make a candidate, copy your baseline to the editor below and change one model instruction.</p>{baseline && <button type="button" onClick={() => onEdit(baseline)}>Copy baseline to editor</button>}
      {baseline && candidate && !diff && <p className="strategy-error">Choose two of your versions with exactly one changed model instruction. Task IDs, order, dependencies and tools must match.</p>}
      {diff && <div className="comparison-diff"><h4>Actual change: {diff.task}.instruction</h4><div className="comparison-columns"><div><b>Baseline</b><pre>{diff.before}</pre></div><div><b>Candidate</b><pre>{diff.after}</pre></div></div></div>}
      <div className="comparison-columns"><label>Provider origin<input type="url" pattern="https?://.+" required maxLength={128} placeholder="https://api.provider.example" value={provider} onChange={e => setProvider(e.target.value)} /></label><label>Model<input required maxLength={128} value={model} onChange={e => setModel(e.target.value)} /></label></div><p>The frozen origin is the scheme, host and port. Set the full completion endpoint separately as ALASHI_MODEL_URL in your local runner.</p><label className="checkbox"><input type="checkbox" checked={test} onChange={e => setTest(e.target.checked)} />Use a test provider with synthetic receipts</label>
      <p>Both sides use this provider and model, temperature 0 and max_tokens 256. Each side starts fresh with the same brief, token budget and deadline. One paired attempt is allowed. Model output may still vary.</p><button disabled={busy || !diff || !challengeId || !hypothesis.trim() || !prediction.trim() || !criterion.trim() || !provider.trim() || !model.trim()}>Freeze comparison</button>
    </form>
    {items.length > 0 && <div className="comparison-saved"><label>Saved comparisons<select value={selected} onChange={e => { setSelected(e.target.value); setReason(''); setNotice('') }}>{items.map(c => <option key={c.id} value={c.id}>{short(c.id)} · {c.content.hypothesis.slice(0, 90)}</option>)}</select></label></div>}
    {current && <div className="comparison-frozen"><span className="eyebrow">FROZEN BEFORE EXECUTION · {short(current.id)}</span><h4>{current.content.hypothesis}</h4><dl><dt>Prediction</dt><dd>{current.content.prediction}</dd><dt>Decision rule</dt><dd>{current.content.decision_criterion}</dd><dt>Changed instruction</dt><dd>{current.actual_diff.task}</dd><dt>Fixed profile</dt><dd>{current.content.profile.model} · {current.content.profile.provider} · temperature 0 · max_tokens 256 · {current.content.profile.source}</dd></dl>
      <details><summary>Actual instruction diff and frozen manifest</summary><div className="comparison-columns"><pre>{current.actual_diff.before}</pre><pre>{current.actual_diff.after}</pre></div><pre>{JSON.stringify(current.content, null, 2)}</pre></details>
      {!attempt && <><p>Configure your local runner and provider first. Preparing the pair starts the shared deadline immediately and uses the single attempt. It does not call a model.</p><button disabled={busy} onClick={() => void perform(async () => { const request = pendingAttempt.current?.comparison === current.id ? pendingAttempt.current : { comparison: current.id, client: crypto.randomUUID() }; pendingAttempt.current = request; setRetryComparison(current.id); const reply = await strategyRequest(`${prefix}/${current.id}/attempts`, { client_id: request.client }); retain(projectComparison(reply.comparison, owner, record)); pendingAttempt.current = null; if (active.current) { setRetryComparison(''); setNotice('Both runs prepared. The shared deadline is running. Execute the command below now.') } })}>{retryComparison === current.id ? 'Retry the same paired attempt' : 'Prepare paired attempt'}</button></>}
      {attempt && <><div className="runner-command"><h4>Run both versions locally</h4><p>Set ALASHI_MODEL_URL, ALASHI_MODEL and ALASHI_MODEL_KEY locally to match the frozen profile. The browser never calls the provider.</p><pre>{`node tools/agent-bootstrap/strategy-run.mjs --profile /path/to/private-profile.json --comparison ${current.id} --attempt-client ${attempt.client_id} --journal-prefix /path/to/private-comparison --allow-provider yes${current.content.profile.source === 'test_provider' ? ' --test-provider yes' : ''}`}</pre><p>To resume, retain the same attempt-client and journal-prefix. The runner permits only the built-in brief_extract and copy_check tools for this comparison.</p></div>
        <div className="comparison-columns"><ComparisonResult label="Baseline" run={attempt.baseline} /><ComparisonResult label="Candidate" run={attempt.candidate} /></div>
        <p className="comparison-limit">Format checks cover the required terms, CTA and word limit. They do not measure clarity, factual quality, conversions or business impact. Token receipts are owner-submitted. One paired attempt is not evidence of a stable improvement.</p>
        {attempt.baseline.test_provider || attempt.candidate.test_provider ? <p className="fixture-banner">TEST PROVIDER. Synthetic token counts do not measure real model consumption.</p> : null}
        {attempt.decision ? <div className="comparison-decision"><h4>Your decision: {outcomes[attempt.decision.outcome]}</h4><p>{attempt.decision.reason}</p></div> : decisionOptions.length ? <form onSubmit={e => { e.preventDefault(); void perform(async () => { await strategyRequest(`${prefix}/${current.id}/attempts/${attempt.id}/decision`, { outcome: chosenOutcome, reason: reason.trim() }); const refreshed = comparisons(await strategyRequest(prefix), owner, record); if (active.current) { setItems(refreshed); setNotice('Your decision is saved with this attempt.') } }) }}><h4>Record your decision</h4><label>Outcome<select value={chosenOutcome} onChange={e => setOutcome(e.target.value as ComparisonDecision['outcome'])}>{decisionOptions.map(key => <option key={key} value={key}>{outcomes[key]}</option>)}</select></label><label>Reason against your original rule<textarea required maxLength={1000} value={reason} onChange={e => setReason(e.target.value)} /></label>{attempt.baseline.unknown_usage || attempt.candidate.unknown_usage ? <p>Usage is unknown. Only an inconclusive decision can be saved.</p> : null}<button disabled={busy || !reason.trim()}>Save owner decision</button></form> : <p>Record your decision after both runs stop and all reserved calls finish. The original rule remains frozen.</p>}
      </>}
      <a download={`alashi-comparison-${short(current.id)}.json`} href={`data:application/json;charset=utf-8,${encodeURIComponent(JSON.stringify(current, null, 2))}`}>Download private comparison receipt</a>
    </div>}
    {error && <p className="strategy-error" role="alert">{error}</p>}{notice && <p role="status">{notice}</p>}
  </section>
}
