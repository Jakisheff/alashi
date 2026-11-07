import { object, LiveApiError } from '../live/api/client.ts'
export type Spec = { id: string; kind: string; depends_on: string[]; instruction?: string; tool: string | null }
export type Version = { id: string; content: { owner: string; parent: string | null; title: string; plan: Spec[]; change: string }; published: boolean; reuseOwners: number }
export type TradeContext = { mode: 'http' | 'solana'; game: string; round: number; phase: string; phase_ends_at: number; participants: {wallet: string; actor: string; cash: string; goods: number}[] }
export type TradeReceipt = { mode: 'http' | 'solana'; game: string; event_id: string; action: string; params: Record<string, unknown>; cash_delta: string; goods_delta: number; confirmed_at: number; signature?: string }
export type Brief = { owner: string; title: string; audience: string; task: string; facts: string[]; required_terms: string[]; cta: string; max_words: number; budget: number; round_seconds: number; trading?: TradeContext }
export type Challenge = { id: string; content: Brief; created_at: number; started_at: number | null; deadline_at: number | null; revealed: boolean }
export type Task = { task: string; status: string; source: string; model: string; reserved: number; usage: { prompt_tokens: number; completion_tokens: number; total_tokens: number; cached_tokens: number } | null; quality_passed: boolean | null; started_at: number; finished_at: number | null }
export type Run = { seat: number; id: string; owner: string; record: string; version: string; challenge: string; plan: Spec[]; brief: Brief; status: string; budget: number; known_tokens: number; reserved_tokens: number; unknown_usage: boolean; test_provider: boolean; created_at: number; calls: Record<string, Task>; deadline_at: number | null; reveal_pending: boolean; result: { artifact: { headline?: string; body?: string; cta?: string; action?: string; params?: Record<string, unknown> }; checks: { passed: boolean; word_count?: number }; finished_at: number; trade_receipt?: TradeReceipt } | null; review: { reviewer: string; accepted: boolean; reason: string } | null }
export type Standing = { challenge: string; groups: { test_provider: boolean; accepted_runs: number; token_leaders: string[]; tokens: number | null }[] }
export type Library = { versions: Version[]; runs: Run[]; challenges: Challenge[]; standings: Standing[]; serverNow: number }
const str = (v: unknown, max = 4096): string => { if (typeof v !== 'string' || v.length > max) throw new LiveApiError('invalid_response'); return v }
const num = (v: unknown): number => { if (!Number.isSafeInteger(v) || Number(v) < 0) throw new LiveApiError('invalid_response'); return Number(v) }
const bool = (v: unknown): boolean => { if (typeof v !== 'boolean') throw new LiveApiError('invalid_response'); return v }
const optionalNum = (v: unknown) => v === null || v === undefined ? null : num(v)
const hash = (v: unknown): string => { const s = str(v, 64); if (!/^[0-9a-f]{64}$/.test(s)) throw new LiveApiError('invalid_response'); return s }
const list = (v: unknown): unknown[] => { if (!Array.isArray(v) || v.length > 500) throw new LiveApiError('invalid_response'); return v }
const strings = (v: unknown) => list(v).map(s => str(s))
function plan(v: unknown): Spec[] { return list(v).map(raw => { const t = object(raw); return { id: str(t.id, 32), kind: str(t.kind, 16), depends_on: strings(t.depends_on), instruction: t.instruction === undefined ? undefined : str(t.instruction, 2000), tool: t.tool === null ? null : str(t.tool, 32) } }) }
function tradeContext(v: unknown): TradeContext {
  const t = object(v), mode = str(t.mode)
  if (mode !== 'http' && mode !== 'solana') throw new LiveApiError('invalid_response')
  return { mode, game: str(t.game, 64), round: num(t.round), phase: str(t.phase, 16), phase_ends_at: num(t.phase_ends_at), participants: list(t.participants).map(raw => {const p = object(raw); return {wallet: str(p.wallet, 64), actor: str(p.actor, 64), cash: str(p.cash, 20), goods: num(p.goods)}}) }
}
function tradeReceipt(v: unknown): TradeReceipt {
  const t = object(v), mode = str(t.mode), cash = str(t.cash_delta, 21)
  if ((mode !== 'http' && mode !== 'solana') || !/^-?(0|[1-9][0-9]*)$/.test(cash) || !Number.isSafeInteger(t.goods_delta)) throw new LiveApiError('invalid_response')
  return {mode, game: str(t.game, 64), event_id: str(t.event_id, 120), action: str(t.action, 20), params: object(t.params), cash_delta: cash, goods_delta: Number(t.goods_delta), confirmed_at: num(t.confirmed_at), ...(mode === 'solana' ? {signature: str(t.signature, 100)} : {})}
}
export const tradingHref = (t: Pick<TradeContext, 'mode' | 'game'>) => t.mode === 'http' ? `/arena?game=${encodeURIComponent(t.game)}` : `/devnet?game=${encodeURIComponent(t.game)}`
export function cashDelta(raw: string) {const n = BigInt(raw), abs = n < 0n ? -n : n; return `${n < 0n ? '-' : '+'}${abs / 1000000n}${abs % 1000000n ? '.' + String(abs % 1000000n).padStart(6, '0').replace(/0+$/, '') : ''}`}
function brief(v: unknown): Brief { const c = object(v); return { owner: str(c.owner, 64), title: str(c.title, 100), audience: str(c.audience, 500), task: str(c.task, 2000), facts: strings(c.facts), required_terms: strings(c.required_terms), cta: str(c.cta, 120), max_words: num(c.max_words), budget: num(c.budget), round_seconds: num(c.round_seconds), ...(c.trading ? {trading: tradeContext(c.trading)} : {}) } }
export function projectVersion(raw: unknown): Version { const v = object(raw), c = object(v.content); return { id: hash(v.id), published: bool(v.published), reuseOwners: num(v.reuse_owners ?? 0), content: { owner: str(c.owner, 64), parent: c.parent === null ? null : hash(c.parent), title: str(c.title, 100), plan: plan(c.plan), change: str(c.change, 1000) } } }
export function projectChallenge(raw: unknown): Challenge { const c = object(raw); return { id: hash(c.id), content: brief(c.content), created_at: num(c.created_at), started_at: optionalNum(c.started_at), deadline_at: optionalNum(c.deadline_at), revealed: c.revealed === undefined ? false : bool(c.revealed) } }
export function projectRun(raw: unknown): Run {
  const r = object(raw), b = brief(r.brief), calls: Record<string, Task> = {}
  for (const [key, value] of Object.entries(object(r.calls))) {
    if (!/^[a-z0-9_-]{1,32}$/.test(key)) throw new LiveApiError('invalid_response')
    const t = object(value), u = t.usage === null ? null : object(t.usage)
    calls[key] = { task: key, status: str(t.status, 32), source: str(t.source, 32), model: str(t.model, 128), reserved: num(t.reserved), usage: u ? { prompt_tokens: num(u.prompt_tokens), completion_tokens: num(u.completion_tokens), total_tokens: num(u.total_tokens), cached_tokens: num(u.cached_tokens) } : null, quality_passed: t.quality_passed === null ? null : bool(t.quality_passed), started_at: num(t.started_at), finished_at: optionalNum(t.finished_at) }
  }
  const result = r.result === null ? null : object(r.result), artifact = result ? object(result.artifact) : null, checks = result ? object(result.checks) : null, review = r.review === null ? null : object(r.review)
  return { seat: num(r.seat), id: hash(r.id), owner: str(r.owner, 64), record: hash(r.record), version: hash(r.version), challenge: hash(r.challenge), plan: plan(r.plan), brief: b, status: str(r.status, 32), budget: num(r.budget), known_tokens: num(r.known_tokens), reserved_tokens: num(r.reserved_tokens), unknown_usage: bool(r.unknown_usage), test_provider: bool(r.test_provider), created_at: num(r.created_at), calls, deadline_at: optionalNum(r.deadline_at), reveal_pending: bool(r.reveal_pending), result: result && artifact && checks ? { artifact: b.trading ? {action: str(artifact.action, 20), params: object(artifact.params)} : { headline: str(artifact.headline), body: str(artifact.body), cta: str(artifact.cta) }, checks: { passed: bool(checks.passed), ...(b.trading ? {} : {word_count: num(checks.word_count)}) }, finished_at: num(result.finished_at), ...(b.trading ? {trade_receipt: tradeReceipt(result.trade_receipt)} : {}) } : null, review: review ? { reviewer: str(review.reviewer, 64), accepted: bool(review.accepted), reason: str(review.reason, 1000) } : null }
}
export function library(raw: unknown): Library { const v = object(raw); return { serverNow: num(v.server_now ?? 0), versions: list(v.versions).map(projectVersion), runs: list(v.runs).map(projectRun), challenges: list(v.challenges).map(projectChallenge), standings: list(v.standings ?? []).map(raw => { const s = object(raw); return { challenge: hash(s.challenge), groups: list(s.groups).map(raw => { const g = object(raw); return { test_provider: bool(g.test_provider), accepted_runs: num(g.accepted_runs), token_leaders: strings(g.token_leaders), tokens: optionalNum(g.tokens) } }) } }) } }
export const strategyPrefix = import.meta.env?.DEV ? '/strategy-api' : ''
export async function strategyRequest(path: string, body?: unknown, signal?: AbortSignal): Promise<Record<string, unknown>> {
  const r = await fetch(strategyPrefix + path, { method: body === undefined ? 'GET' : 'POST', credentials: 'same-origin', redirect: 'error', cache: 'no-store', signal: signal ? AbortSignal.any([signal, AbortSignal.timeout(10_000)]) : AbortSignal.timeout(10_000), headers: { Accept: 'application/json', ...(body === undefined ? {} : { 'Content-Type': 'application/json' }) }, ...(body === undefined ? {} : { body: JSON.stringify(body) }) })
  const v = object(await r.json()); if (!r.ok || v.ok !== true) throw new LiveApiError(typeof v.error === 'string' && /^[a-z_]{1,80}$/.test(v.error) ? v.error : 'unavailable'); return v
}
export function errorText(e: unknown) {
  const messages: Record<string, string> = { publish_derivative_first: 'Publish this derivative before joining.', host_cannot_compete: 'The host cannot compete in this duel.', equal_active_trading_conditions_required: 'Choose a market or production phase with two owners starting with equal cash and goods.', trading_window_unavailable: 'Both players need an unused action and enough time in the current phase.', trading_snapshot_unavailable: 'The game service must expose current trading conditions before a challenge can start.', trading_owner_forbidden: 'Only the two owners in this game can enter this trading round.', duel_seat_unavailable: 'Both seats are taken, or this owner already joined.', waiting_for_opponent: 'Waiting for the second owner.', round_ended: 'The round ended. New calls are stopped; finished calls still count.', budget_exhausted: 'The remaining token budget cannot cover this reservation.', owner_session_invalid: 'Verify ownership again.', owner_session_expired: 'Your owner session expired. Verify again.', storage_failed: 'The write was not saved. Retry the same operation.', idempotency_conflict: 'This request ID belongs to a different operation.', strategy_capacity: 'This pilot has reached its storage limit.' }
  const comparisonMessages: Record<string, string> = {
    bad_comparison: 'Check the comparison fields and fixed provider profile. Text limits are measured in UTF-8 bytes.',
    comparison_forbidden: 'This comparison belongs to a different owner or agent record.',
    comparison_requires_static_ad_brief: 'Choose an existing advertisement brief without a trading context.',
    comparison_strategy_not_owned: 'Both strategy versions must belong to you.',
    comparison_plan_structure_changed: 'Keep task IDs, order, dependencies and tools identical.',
    comparison_tool_not_allowed: 'This comparison supports only the built-in brief_extract and copy_check tools.',
    comparison_requires_one_instruction_change: 'Change exactly one model instruction between versions.',
    comparison_requires_model_instruction_change: 'The changed instruction must belong to a model task. Built-in tool instructions cannot be the intervention.',
    comparison_attempt_limit: 'This comparison has used its allowed attempts. Reopen the existing receipt.',
    comparison_provider_mismatch: 'Use the provider, model and settings frozen in this comparison.',
    comparison_pair_incomplete: 'Wait for both runs to stop and all reserved calls to finish. Unknown usage permits only an inconclusive decision.',
    bad_comparison_decision: 'Choose an available outcome and explain your decision.',
    comparison_decision_frozen: 'This owner decision is already frozen. Reopen the saved receipt.',
  }
  return e instanceof LiveApiError ? comparisonMessages[e.code] ?? messages[e.code] ?? 'The operation could not be completed. Check the service and owner session.' : 'The service could not be reached. Last confirmed public data is retained.'
}

export type InstructionDiff = { task: string; field: 'instruction'; before: string; after: string }
export type ComparisonProfile = { source: 'provider_reported' | 'test_provider'; provider: string; model: string; temperature: 0; max_tokens: 256 }
export type ComparisonDecision = { outcome: 'keep_baseline' | 'adopt_candidate' | 'inconclusive'; reason: string; at: number }
export type ComparisonAttempt = { id: string; number: number; client_id: string; baseline: Run; candidate: Run; decision: ComparisonDecision | null }
export type Comparison = {
  id: string
  content: { record: string; owner: string; challenge_id: string; baseline_version: string; candidate_version: string; hypothesis: string; prediction: string; decision_criterion: string; changed_task: string; profile: ComparisonProfile; attempt_limit: number }
  actual_diff: InstructionDiff
  attempts: ComparisonAttempt[]
}

export function comparisonDecisionOptions(attempt: ComparisonAttempt, now: number): ComparisonDecision['outcome'][] {
  const runs = [attempt.baseline, attempt.candidate]
  if (runs.some(r => Object.values(r.calls).some(c => c.status === 'reserved') || r.status === 'running' && (r.deadline_at === null || now < r.deadline_at))) return []
  if (runs.some(r => r.unknown_usage)) return ['inconclusive']
  return [...(attempt.baseline.status === 'completed' ? ['keep_baseline' as const] : []), ...(attempt.candidate.status === 'completed' ? ['adopt_candidate' as const] : []), 'inconclusive']
}

export function instructionDiff(baseline: Version, candidate: Version): InstructionDiff | null {
  const a = baseline.content.plan, b = candidate.content.plan
  if (baseline.id === candidate.id || baseline.content.owner !== candidate.content.owner || a.length !== b.length) return null
  const changes: InstructionDiff[] = []
  for (let i = 0; i < a.length; i++) {
    if (a[i].id !== b[i].id || a[i].kind !== b[i].kind || a[i].tool !== b[i].tool || JSON.stringify(a[i].depends_on) !== JSON.stringify(b[i].depends_on)) return null
    if (a[i].instruction !== b[i].instruction) {
      if (a[i].kind !== 'model' || typeof a[i].instruction !== 'string' || typeof b[i].instruction !== 'string') return null
      changes.push({ task: a[i].id, field: 'instruction', before: a[i].instruction!, after: b[i].instruction! })
    }
  }
  return changes.length === 1 ? changes[0] : null
}

export function projectComparison(raw: unknown, expectedOwner: string, expectedRecord?: string): Comparison {
  const v = object(raw), c = object(v.content), d = object(v.actual_diff), p = object(c.profile)
  const owner = str(c.owner, 64), source = str(p.source, 32)
  if (owner !== expectedOwner || !['provider_reported', 'test_provider'].includes(source) || p.temperature !== 0 || p.max_tokens !== 256 || !Number.isInteger(c.attempt_limit) || Number(c.attempt_limit) < 1 || Number(c.attempt_limit) > 5 || d.field !== 'instruction') throw new LiveApiError('invalid_response')
  const content: Comparison['content'] = {
    record: hash(c.record), owner, challenge_id: hash(c.challenge_id), baseline_version: hash(c.baseline_version), candidate_version: hash(c.candidate_version), hypothesis: str(c.hypothesis, 2000), prediction: str(c.prediction, 2000), decision_criterion: str(c.decision_criterion, 2000), changed_task: str(c.changed_task, 32), profile: { source: source as ComparisonProfile['source'], provider: str(p.provider, 128), model: str(p.model, 128), temperature: 0, max_tokens: 256 }, attempt_limit: num(c.attempt_limit),
  }
  const diff: InstructionDiff = { task: str(d.task, 32), field: 'instruction', before: str(d.before, 2000), after: str(d.after, 2000) }
  if (expectedRecord !== undefined && content.record !== expectedRecord || diff.task !== content.changed_task || diff.before === diff.after || content.baseline_version === content.candidate_version) throw new LiveApiError('invalid_response')
  const attempts = list(v.attempts).map(rawAttempt => {
    const a = object(rawAttempt), baseline = projectRun(a.baseline), candidate = projectRun(a.candidate), decision = a.decision === null ? null : object(a.decision)
    if ([baseline, candidate].some(r => r.owner !== owner || r.record !== content.record || r.challenge !== content.challenge_id || r.brief.trading) || baseline.version !== content.baseline_version || candidate.version !== content.candidate_version || baseline.id === candidate.id) throw new LiveApiError('invalid_response')
    const outcome = decision ? str(decision.outcome, 32) : null
    const parsed: ComparisonAttempt = { id: hash(a.id), number: num(a.number), client_id: str(a.client_id, 64), baseline, candidate, decision: decision ? { outcome: outcome as ComparisonDecision['outcome'], reason: str(decision.reason, 2000), at: num(decision.at) } : null }
    if (decision && !comparisonDecisionOptions(parsed, parsed.decision!.at).includes(outcome as ComparisonDecision['outcome'])) throw new LiveApiError('invalid_response')
    if (JSON.stringify(baseline.brief) !== JSON.stringify(candidate.brief) || baseline.budget !== candidate.budget || baseline.deadline_at !== candidate.deadline_at) throw new LiveApiError('invalid_response')
    const version = (r: Run): Version => ({ id: r.version, published: false, reuseOwners: 0, content: { owner: r.owner, parent: null, title: '', change: '', plan: r.plan } })
    if (JSON.stringify(instructionDiff(version(baseline), version(candidate))) !== JSON.stringify(diff)) throw new LiveApiError('invalid_response')
    return parsed
  })
  if (attempts.length > content.attempt_limit || attempts.some((a, i) => a.number !== i + 1)) throw new LiveApiError('invalid_response')
  return { id: hash(v.id), content, actual_diff: diff, attempts }
}

export function comparisons(raw: unknown, owner: string, record?: string): Comparison[] {
  return list(object(raw).comparisons).map(v => projectComparison(v, owner, record))
}
