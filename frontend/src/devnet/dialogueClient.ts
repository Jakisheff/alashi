import { useEffect, useState } from 'react'
import { ChainApiError, validBase58, type ChainSnapshot } from './client.ts'
export type DialogueEntry = {
  message_id: string; seq: string; game_pda: string; author_faction_pda: string; to_faction_pda: string
  text: string; reply_to_message_id: string | null; related_offer_entry_id: string | null
  round: number; phase: string; phase_ends_at: number; created_at: string
  observed_chain_event_id: string | null; source: 'runner_reported'
}
export type DialoguePage = { entries: DialogueEntry[]; next_cursor: string; latest_seq: string; has_more: boolean }
const invalid = (): never => { throw new ChainApiError('invalid') }
const str = (v: unknown, max = 160): string => typeof v === 'string' && v.length > 0 && [...v].length <= max ? v : invalid()
const hex = (v: unknown): string => { const s = str(v, 64); return /^[a-f0-9]{64}$/.test(s) ? s : invalid() }
const optional = (v: unknown): string | null => v === null ? null : hex(v)
const conversationU64 = (v: unknown): string => { const s = str(v, 20); return /^(0|[1-9][0-9]*)$/.test(s) && BigInt(s) <= 18446744073709551615n ? s : invalid() }
export function decodeDialogue(raw: unknown, game: string, after = '0'): DialoguePage {
  if (!raw || typeof raw !== 'object' || Array.isArray(raw)) return invalid()
  const v = raw as Record<string, unknown>
  let previous = BigInt(conversationU64(after)); const ids = new Set<string>()
  if (v.ok !== true || v.schema !== 'alashi.chain_dialogue.v1' || v.game_pda !== game || !validBase58(game)
    || !Array.isArray(v.entries) || v.entries.length > 100) return invalid()
  const entries = v.entries.map((raw): DialogueEntry => {
    if (!raw || typeof raw !== 'object' || Array.isArray(raw)) return invalid()
    const e = raw as Record<string, unknown>, id = hex(e.message_id), seq = conversationU64(e.seq)
    const author = str(e.author_faction_pda, 44), to = str(e.to_faction_pda, 44), created = str(e.created_at, 64)
    if (e.game_pda !== game || e.source !== 'runner_reported' || ids.has(id) || BigInt(seq) <= previous
      || !validBase58(author) || !validBase58(to) || author === to || !Number.isInteger(e.round) || Number(e.round) < 1 || Number(e.round) > 6
      || !['Market', 'Action', 'Law'].includes(String(e.phase))
      || !Number.isSafeInteger(e.phase_ends_at) || !Number.isFinite(Date.parse(created)) || 'receipt' in e) return invalid()
    const content = str(e.text, 240)
    if (!content.trim() || /[\u0000-\u001f\u007f-\u009f]/.test(content)) return invalid()
    previous = BigInt(seq); ids.add(id)
    return { message_id: id, seq, game_pda: game, author_faction_pda: author, to_faction_pda: to, text: content,
      reply_to_message_id: optional(e.reply_to_message_id), related_offer_entry_id: optional(e.related_offer_entry_id),
      round: Number(e.round), phase: String(e.phase), phase_ends_at: Number(e.phase_ends_at), created_at: created,
      observed_chain_event_id: e.observed_chain_event_id === null ? null : str(e.observed_chain_event_id), source: 'runner_reported' }
  })
  const cursor = conversationU64(v.next_cursor), latest = conversationU64(v.latest_seq)
  if (BigInt(cursor) !== previous || BigInt(latest) < previous || typeof v.has_more !== 'boolean'
    || v.has_more !== (BigInt(latest) > previous) || (v.has_more && !entries.length)) return invalid()
  return { entries, next_cursor: cursor, latest_seq: latest, has_more: v.has_more }
}
export async function dialoguePage(game: string, after: string, signal: AbortSignal): Promise<DialoguePage> {
  if (!validBase58(game)) return invalid()
  conversationU64(after)
  const response = await fetch(`/chain/devnet/games/${encodeURIComponent(game)}/dialogue?after=${after}&limit=100`, {
    credentials: 'omit', redirect: 'error', cache: 'no-store', referrerPolicy: 'no-referrer', headers: { Accept: 'application/json' },
    signal: AbortSignal.any([signal, AbortSignal.timeout(20_000)]),
  })
  if (response.status === 429) throw new ChainApiError('rate_limit', 30_000)
  if (response.status === 404) throw new ChainApiError('not_found')
  if (!response.ok) throw new ChainApiError('unavailable')
  const raw = await response.text(); if (raw.length > 256_000) return invalid()
  try { return decodeDialogue(JSON.parse(raw), game, after) } catch { return invalid() }
}
export function useChainDialogue(game: string, enabled: boolean) {
  const [data, setData] = useState<{ game: string; entries: DialogueEntry[]; loaded: boolean; error: string }>({ game, entries: [], loaded: false, error: '' })
  const [revision, setRevision] = useState(0)
  useEffect(() => {
    let stopped = false, timer = 0, cursor = '0', entries: DialogueEntry[] = [], failures = 0
    const controller = new AbortController()
    setData({ game, entries: [], loaded: false, error: '' })
    if (!enabled) return () => controller.abort()
    const poll = async () => {
      if (stopped) return
      let delay = 5000
      try {
        const page = await dialoguePage(game, cursor, controller.signal)
        if (stopped) return
        const known = new Map(entries.map((e) => [e.message_id, e]))
        for (const e of page.entries) {
          const previous = known.get(e.message_id)
          if (previous && JSON.stringify(previous) !== JSON.stringify(e)) return invalid()
          known.set(e.message_id, e)
        }
        if (known.size > 10_000) return invalid()
        entries = [...known.values()]; cursor = page.next_cursor; failures = 0
        setData({ game, entries, loaded: true, error: '' }); delay = page.has_more ? 0 : 5000
      } catch (e) {
        if (stopped) return
        const error = e instanceof ChainApiError && e.code === 'not_found' ? 'Dialogue is not available for this game yet.'
          : e instanceof ChainApiError && e.code === 'invalid' ? 'Dialogue data could not be validated.' : 'Dialogue connection interrupted.'
        setData((old) => ({ game, entries, loaded: old.game === game && old.loaded, error }))
        failures++; delay = Math.max(12_000 * 2 ** Math.min(failures - 1, 4), e instanceof ChainApiError ? e.retryMs : 0)
      }
      if (!stopped) timer = window.setTimeout(() => void poll(), delay)
    }
    void poll()
    return () => { stopped = true; controller.abort(); window.clearTimeout(timer) }
  }, [game, enabled, revision])
  return { entries: data.game === game ? data.entries : [], loaded: data.game === game && data.loaded,
    error: data.game === game ? data.error : '', retry: () => setRevision((n) => n + 1) }
}
export function dialogueView(entries: readonly DialogueEntry[], snapshot: ChainSnapshot, replayEvent: string | null) {
  const indices = new Map(snapshot.events.map((e, i) => [e.id, i]))
  const stop = replayEvent === null ? snapshot.events.length - 1 : (indices.get(replayEvent) ?? -1)
  const factions = new Set(snapshot.factions.map((f) => f.pda))
  const valid = entries.filter((e) => e.game_pda === snapshot.pda && factions.has(e.author_faction_pda) && factions.has(e.to_faction_pda))
  return { timeline: valid.filter((e) => e.observed_chain_event_id !== null && indices.has(e.observed_chain_event_id) && indices.get(e.observed_chain_event_id)! <= stop),
    separate: valid.filter((e) => e.observed_chain_event_id === null || !indices.has(e.observed_chain_event_id)) }
}
