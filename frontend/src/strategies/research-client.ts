import { LiveApiError, object } from '../live/api/client.ts'
import { errorText } from './client.ts'

export type BacklogItem = { id: string; client_id: string; title: string; hypothesis: string; priority: 'now' | 'later'; comparison_id: string | null; source_card_id: string | null; created_at: number; updated_at: number }
export type ForecastSummary = { submitted_count: number; missing_count: number; min_probability_bps: number | null; max_probability_bps: number | null; mean_probability_bps: number | null; mean_rounding: 'floor' }
export type Forecast = { probability_bps: number; reason: string; submitted_at: number }
export type ResearchRound = { comparison_id: string; hypothesis: string; decision_criterion: string; is_owner: boolean; participants: string[]; submitted_count: number; missing_count: number; created_at: number; closed_at: number | null; own_forecast: Forecast | null; forecasts: (Forecast & { wallet: string })[] | null; summary: ForecastSummary | null; forecast_subject: 'candidate_satisfies_decision_criterion' }
export type MechanismCard = { id: string; title: string; mechanism: string; task: string; conditions: string; result: string; limitations: string; evidence_status: 'model' | 'owner_reported_local'; license: 'CC0-1.0'; created_at: number }
export type PublicationDraft = Pick<MechanismCard, 'title' | 'mechanism' | 'task' | 'conditions' | 'result' | 'limitations'>
export type LocalPublication = PublicationDraft & { client_id: string; license: 'CC0-1.0' }
export type ResearchWorkspace = { backlog: BacklogItem[]; rounds: ResearchRound[]; applied: { comparison_id: string; attempt_id: string; version_id: string; applied_at: number } | null; digest: { fingerprint: string; day: string; timezone: 'Asia/Almaty'; unread: boolean; items: { kind: string; comparison_id: string; attempt_id: string | null; title: string; at: number; summary: ForecastSummary | null }[]; next_actions: { kind: string; comparison_id: string | null; attempt_id: string | null; title: string }[] }; library: MechanismCard[] }

const invalid = () => new LiveApiError('invalid_response')
const text = (value: unknown, max = 4000): string => { if (typeof value !== 'string' || new TextEncoder().encode(value).length > max) throw invalid(); return value }
const integer = (value: unknown): number => { if (!Number.isSafeInteger(value) || Number(value) < 0) throw invalid(); return Number(value) }
const boolean = (value: unknown): boolean => { if (typeof value !== 'boolean') throw invalid(); return value }
const array = (value: unknown, max = 500): unknown[] => { if (!Array.isArray(value) || value.length > max) throw invalid(); return value }
const hash = (value: unknown): string => { const id = text(value, 64); if (!/^[a-f0-9]{64}$/.test(id)) throw invalid(); return id }
const nullableHash = (value: unknown) => value === null ? null : hash(value)
const timestamp = (value: unknown) => value === null ? null : integer(value)
function forecast(raw: unknown): Forecast { const f = object(raw), probability = integer(f.probability_bps); if (probability > 10000) throw invalid(); return { probability_bps: probability, reason: text(f.reason, 2000), submitted_at: integer(f.submitted_at) } }

function forecastSummary(raw: unknown): ForecastSummary | null {
  if (raw === null || raw === undefined) return null
  const s = object(raw), submitted = integer(s.submitted_count), missing = integer(s.missing_count), min = timestamp(s.min_probability_bps), max = timestamp(s.max_probability_bps), mean = timestamp(s.mean_probability_bps)
  if (s.mean_rounding !== 'floor' || submitted === 0 && [min, max, mean].some(v => v !== null) || submitted > 0 && [min, max, mean].some(v => v === null) || min !== null && max !== null && mean !== null && (min > max || max > 10000 || mean < min || mean > max)) throw invalid()
  return { submitted_count: submitted, missing_count: missing, min_probability_bps: min, max_probability_bps: max, mean_probability_bps: mean, mean_rounding: 'floor' }
}

export function publicationDraft(raw: unknown): PublicationDraft {
  const c = object(raw)
  return { title: text(c.title, 100), mechanism: text(c.mechanism, 2000), task: text(c.task, 2000), conditions: text(c.conditions, 2000), result: text(c.result, 4000), limitations: text(c.limitations, 4000) }
}
export function localPublication(raw: unknown): LocalPublication {
  const c = object(raw), client = text(c.client_id, 80)
  if (c.license !== 'CC0-1.0' || !/^[A-Za-z0-9_-]{8,80}$/.test(client)) throw invalid()
  // Import only the reviewed card fields. A file cannot carry permission to publish.
  return { ...publicationDraft(c), client_id: client, license: 'CC0-1.0' }
}
export function mechanismCard(raw: unknown): MechanismCard {
  const c = object(raw), evidence = text(c.evidence_status, 32)
  if (c.license !== 'CC0-1.0' || !['model', 'owner_reported_local'].includes(evidence)) throw invalid()
  return { ...publicationDraft(c), id: hash(c.id), evidence_status: evidence as MechanismCard['evidence_status'], license: 'CC0-1.0', created_at: integer(c.created_at) }
}
export function researchLibrary(raw: unknown): MechanismCard[] { return array(object(raw).cards).map(mechanismCard) }
export function researchWorkspace(raw: unknown, wallet: string): ResearchWorkspace {
  const r = object(raw), digest = object(r.digest), applied = r.applied === null ? null : object(r.applied), day = text(digest.day, 10)
  if (!/^\d{4}-\d{2}-\d{2}$/.test(day) || digest.timezone !== 'Asia/Almaty') throw invalid()
  return {
    backlog: array(r.backlog).map(raw => { const b = object(raw), priority = text(b.priority, 10); if (priority !== 'now' && priority !== 'later') throw invalid(); return { id: hash(b.id), client_id: text(b.client_id, 80), title: text(b.title, 100), hypothesis: text(b.hypothesis, 2000), priority, comparison_id: nullableHash(b.comparison_id), source_card_id: nullableHash(b.source_card_id), created_at: integer(b.created_at), updated_at: integer(b.updated_at) } }),
    rounds: array(r.rounds).map(raw => {
      const f = object(raw), participants = array(f.participants).map(v => text(v, 64)), closed = timestamp(f.closed_at), submitted = integer(f.submitted_count), missing = integer(f.missing_count)
      const summary = forecastSummary(f.summary)
      const own = f.own_forecast === null ? null : forecast(f.own_forecast), forecasts = f.forecasts === null ? null : array(f.forecasts).map(raw => ({ ...forecast(raw), wallet: text(object(raw).wallet, 64) }))
      if (participants.length > 32 || new Set(participants).size !== participants.length || submitted + missing !== participants.length || closed === null && forecasts !== null || closed !== null && forecasts === null || forecasts && (forecasts.length !== submitted || new Set(forecasts.map(v => v.wallet)).size !== forecasts.length || forecasts.some(v => !participants.includes(v.wallet))) || own && !participants.includes(wallet)) throw invalid()
      if (f.forecast_subject !== 'candidate_satisfies_decision_criterion' || closed === null && summary !== null || closed !== null && (!summary || summary.submitted_count !== submitted || summary.missing_count !== missing)) throw invalid()
      if (forecasts && forecasts.length && summary && (summary.min_probability_bps !== Math.min(...forecasts.map(f => f.probability_bps)) || summary.max_probability_bps !== Math.max(...forecasts.map(f => f.probability_bps)) || summary.mean_probability_bps !== Math.floor(forecasts.reduce((sum, f) => sum + f.probability_bps, 0) / forecasts.length))) throw invalid()
      return { summary, forecast_subject: 'candidate_satisfies_decision_criterion', comparison_id: hash(f.comparison_id), hypothesis: text(f.hypothesis, 2000), decision_criterion: text(f.decision_criterion, 2000), is_owner: boolean(f.is_owner), participants, submitted_count: submitted, missing_count: missing, created_at: integer(f.created_at), closed_at: closed, own_forecast: own, forecasts }
    }),
    applied: applied ? { comparison_id: hash(applied.comparison_id), attempt_id: hash(applied.attempt_id), version_id: hash(applied.version_id), applied_at: integer(applied.applied_at) } : null,
    digest: { fingerprint: hash(digest.fingerprint), day, timezone: 'Asia/Almaty', unread: boolean(digest.unread), items: array(digest.items, 4000).map(raw => { const i = object(raw); return { kind: text(i.kind, 64), comparison_id: hash(i.comparison_id), attempt_id: nullableHash(i.attempt_id), title: text(i.title, 2000), at: integer(i.at), summary: forecastSummary(i.summary) } }), next_actions: array(digest.next_actions, 4000).map(raw => { const i = object(raw); return { kind: text(i.kind, 64), comparison_id: nullableHash(i.comparison_id), attempt_id: nullableHash(i.attempt_id ?? null), title: text(i.title, 2000) } }) },
    library: array(r.library).map(mechanismCard),
  }
}

export function researchError(error: unknown): string {
  const messages: Record<string, string> = {
    bad_research_request: 'Check the action fields and retry.',
    bad_research_backlog: 'Give this idea a title and hypothesis, then choose Now or Later.',
    bad_forecast_participants: 'Invite between 1 and 32 registered wallets, one address per line.',
    forecast_round_frozen: 'The invited wallets are fixed for this round.',
    forecast_requires_unstarted_comparison: 'Forecasts must be fixed before the comparison starts.',
    forecast_required: 'Submit a forecast before continuing.',
    unknown_forecast_round: 'This comparison has no forecast round yet.',
    digest_changed: 'The summary changed while you were reading. Review the latest updates and mark them as read again.',
    digest_day_changed: 'A new daily summary is available. Refresh and review it before marking it as read.',
    comparison_decision_required: 'Complete the comparison and record your decision first.',
    comparison_decision_inconclusive: 'An inconclusive decision cannot select a new default.',
    bad_mechanism_card: 'Complete all public fields, review them and confirm CC0 permission. Text limits are measured in UTF-8 bytes.',
    publication_frozen: 'This public card is already fixed and cannot be overwritten.',
    unknown_mechanism_card: 'This public card is no longer available.',
    bad_research: 'Check the fields. Text limits are measured in UTF-8 bytes.',
    bad_backlog: 'Give this idea a title and hypothesis, then choose Now or Later.',
    bad_forecast: 'Enter a probability from 0 to 100 and a reason.',
    forecast_frozen: 'Your forecast is already saved and cannot be edited.',
    forecast_round_closed: 'This round is closed. Forecasts cannot be added.',
    forecast_round_missing: 'Open a forecast round for this comparison first.',
    forecast_round_exists: 'This comparison already has a forecast round.',
    forecast_forbidden: 'Only invited wallets can submit a forecast.',
    comparison_forecasts_open: 'Close and reveal the forecast round before preparing a comparison attempt.',
    forecast_round_open: 'Close and reveal the forecast round before preparing a comparison attempt.',
    forecast_after_execution: 'Forecasts must be fixed before the comparison starts.',
    research_forbidden: 'This action is not available to your wallet.',
    application_requires_decision: 'Save a decision to keep the baseline or adopt the candidate first.',
    publication_requires_decision: 'Save the owner decision before publishing a mechanism card.',
    publication_consent_required: 'Review the public text and confirm your right to share it under CC0.',
    bad_publication: 'Complete every public field and confirm the CC0 permission.',
  }
  return error instanceof LiveApiError ? messages[error.code] ?? errorText(error) : errorText(error)
}
