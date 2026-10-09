import { validRecord } from '../live/api/client'
import { validBase58 } from '../devnet/client'

export type OwnerLocator = { record: string; game: string; faction: string }
const LOCATOR_STORAGE_NAME = 'alashi.owner.locator.v1'
const PENDING_STORAGE_NAME = 'alashi.owner.pairing.pending.v1'
const code = /^[0-9a-f]{64}$/

function object(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('invalid_response')
  return value as Record<string, unknown>
}
async function post(path: string, body: Record<string, unknown>): Promise<Record<string, unknown>> {
  const response = await fetch(path, {
    method: 'POST', credentials: 'include', redirect: 'error', cache: 'no-store',
    referrerPolicy: 'no-referrer', headers: { Accept: 'application/json', 'Content-Type': 'application/json' },
    body: JSON.stringify(body), signal: AbortSignal.timeout(10_000),
  })
  const value = object(await response.json())
  if (!response.ok || value.ok !== true) throw new Error(typeof value.error === 'string' ? value.error : 'unavailable')
  return value
}
export function readOwnerLocator(): OwnerLocator | null {
  try {
    const value = object(JSON.parse(localStorage.getItem(LOCATOR_STORAGE_NAME) ?? 'null'))
    if (typeof value.record === 'string' && typeof value.game === 'string' && typeof value.faction === 'string'
      && validRecord(value.record) && validBase58(value.game) && validBase58(value.faction)) {
      return { record: value.record as string, game: value.game as string, faction: value.faction as string }
    }
  } catch { /* no storage or no previous player */ }
  return null
}
export function rememberOwnerLocator(locator: OwnerLocator) {
  if (!validRecord(locator.record) || !validBase58(locator.game) || !validBase58(locator.faction)) return
  try { localStorage.setItem(LOCATOR_STORAGE_NAME, JSON.stringify(locator)) } catch { /* private browsing */ }
}
export function forgetOwnerLocator() {
  try { localStorage.removeItem(LOCATOR_STORAGE_NAME) } catch { /* private browsing */ }
}
export async function ownerSessionExists(record: string): Promise<boolean> {
  if (!validRecord(record)) return false
  try {
    const response = await fetch(`/agents/${record}/owner/browser/session`, {
      credentials: 'include', redirect: 'error', cache: 'no-store', referrerPolicy: 'no-referrer',
      signal: AbortSignal.timeout(10_000),
    })
    return response.ok && object(await response.json()).ok === true
  } catch { return false }
}
export async function startOwnerPairing(expectedRecord?: string) {
  // This marker contains no grant or credential. It lets the same tab resume
  // polling after a reload, including when the start response was lost.
  rememberPendingPairing(Math.floor(Date.now() / 1000) + 1800, expectedRecord)
  const value = await post('/owner/pairing/start', expectedRecord ? { expected_record_id: expectedRecord } : {})
  if (typeof value.pairing_grant !== 'string' || !code.test(value.pairing_grant) || !Number.isSafeInteger(value.expires_at)) throw new Error('invalid_response')
  rememberPendingPairing(value.expires_at as number, expectedRecord)
  return { grant: value.pairing_grant, expiresAt: value.expires_at as number }
}
export function rememberPendingPairing(expiresAt: number, expectedRecord?: string) {
  try { sessionStorage.setItem(PENDING_STORAGE_NAME, JSON.stringify({ expiresAt, expectedRecord })) } catch { /* private browsing */ }
}
export function readPendingPairing(): { expiresAt: number; expectedRecord?: string } | null {
  try {
    const value = object(JSON.parse(sessionStorage.getItem(PENDING_STORAGE_NAME) ?? 'null'))
    if (Number.isSafeInteger(value.expiresAt) && (value.expiresAt as number) * 1000 > Date.now()
      && (value.expectedRecord === undefined || (typeof value.expectedRecord === 'string' && validRecord(value.expectedRecord)))) {
      return { expiresAt: value.expiresAt as number, expectedRecord: value.expectedRecord as string | undefined }
    }
  } catch { /* no prior pairing */ }
  forgetPendingPairing()
  return null
}
export function forgetPendingPairing() {
  try { sessionStorage.removeItem(PENDING_STORAGE_NAME) } catch { /* private browsing */ }
}
export async function pollOwnerPairing(): Promise<{ status: 'waiting'; expiresAt: number } | { status: 'paired'; locator: OwnerLocator }> {
  const value = await post('/owner/pairing/status', {})
  if (value.status === 'waiting' && Number.isSafeInteger(value.expires_at)) return { status: 'waiting', expiresAt: value.expires_at as number }
  if (value.status === 'paired' && typeof value.agent_record_id === 'string' && typeof value.game_pda === 'string'
    && typeof value.faction_pda === 'string' && validRecord(value.agent_record_id)
    && validBase58(value.game_pda) && validBase58(value.faction_pda)) {
    return { status: 'paired', locator: { record: value.agent_record_id as string, game: value.game_pda as string, faction: value.faction_pda as string } }
  }
  throw new Error('invalid_response')
}
/** Strip the fragment before any network request or component effect. */
export function consumeOwnerFragment(): { record: string; code: string } | null {
  const hash = window.location.hash
  if (!hash.startsWith('#owner=')) return null
  window.history.replaceState(window.history.state, '', window.location.pathname + window.location.search)
  const match = /^#owner=([0-9a-f]{64})\.([0-9a-f]{64})$/.exec(hash)
  return match ? { record: match[1], code: match[2] } : null
}
export async function redeemOwnerFragment(record: string, handoffCode: string, game: string): Promise<OwnerLocator> {
  if (!validRecord(record) || !code.test(handoffCode) || !validBase58(game)) throw new Error('invalid_link')
  const value = await post(`/agents/${record}/owner/browser/handoff`, { code: handoffCode, game_pda: game })
  if (value.agent_record_id !== record || value.game_pda !== game || typeof value.faction_pda !== 'string'
    || !validBase58(value.faction_pda)) throw new Error('invalid_response')
  return { record, game, faction: value.faction_pda as string }
}
