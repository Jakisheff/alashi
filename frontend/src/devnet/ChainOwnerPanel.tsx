import { useCallback, useEffect, useRef, useState } from 'react'
import { LiveApiError, friendlyError, type OwnerSession } from '../live/api/client'
import { availableWallets, signOwnerChallenge, type Wallet } from '../live/api/wallet'

import { createChainOwnerApi, mergeChainWishes, type ChainOwnerApi, type ChainWish as Wish, type ChainPending as Pending, type ChainIntent } from './ownerClient'
import type { ChainFaction } from './client'
import '../live/stream.css'
const defaultApi = createChainOwnerApi(import.meta.env.VITE_LIVE_API_PATH ?? '')
const intentLabels: Record<ChainIntent, string> = { produce: 'Produce goods', sell_one: 'Sell one good', buy_one: 'Buy one good', vote_yes: 'Vote Yes', vote_no: 'Vote No' }
type Access = 'restoring' | 'guest' | 'owner'
const statusLabels: Record<Wish['status'], string> = { received: 'Received privately', consumed: 'Agent considering it', unconfirmed: 'Awaiting chain confirmation', confirmed: 'Confirmed on-chain', deferred: 'Saved for later', declined: 'Not used', expired: 'Game ended' }
const short = (value: string) => `${value.slice(0, 4)}…${value.slice(-4)}`
const sessionTime = (seconds: number) => new Date(seconds * 1000).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' })

export function ChainOwnerPanel({ api = defaultApi, record, gamePda, faction }: { api?: ChainOwnerApi; record: string; gamePda: string; faction: ChainFaction }) {
  // The HttpOnly cookie is never visible here. Drafts and private history stay in memory.
  const generation = useRef(0)
  const authRequest = useRef<AbortController | null>(null)
  const journalRequest = useRef<AbortController | null>(null)
  const submitting = useRef(false)
  const wallet = useRef<Wallet | null>(null)
  const [access, setAccess] = useState<Access>('restoring')
  const [session, setSession] = useState<OwnerSession | null>(null)
  const [connectedWallet, setConnectedWallet] = useState('')
  const [busy, setBusy] = useState(false)
  const [sending, setSending] = useState(false)
  const [error, setError] = useState('')
  const [notice, setNotice] = useState('')
  const [draft, setDraft] = useState('')
  const [pending, setPending] = useState<Pending | null>(null)
  const [wishes, setWishes] = useState<Wish[]>([])
  const [remaining, setRemaining] = useState<number | null>(null)
  const [intent, setIntent] = useState<ChainIntent>('produce')
  const [bound, setBound] = useState(false)
  const [refresh, setRefresh] = useState(0)
  const [journalReady, setJournalReady] = useState(false)
  const [journalOnline, setJournalOnline] = useState(false)
  const [walletChoice, setWalletChoice] = useState(0)
  const [wallets, setWallets] = useState(availableWallets)
  const [logoutRetry, setLogoutRetry] = useState(false)

  const invalidatePrivate = useCallback(() => {
    generation.current++; authRequest.current?.abort(); journalRequest.current?.abort(); submitting.current = false; wallet.current = null
  }, [])
  const clearPrivate = useCallback((message = '') => {
    invalidatePrivate()
    setAccess('guest'); setSession(null); setConnectedWallet(''); setBusy(false); setSending(false); setDraft(''); setPending(null); setWishes([]); setRemaining(null); setIntent('produce'); setBound(false); setError(message); setNotice(''); setJournalReady(false); setJournalOnline(false)
  }, [invalidatePrivate])

  const restore = useCallback(async () => {
    generation.current++; authRequest.current?.abort(); journalRequest.current?.abort()
    const ticket = generation.current, controller = new AbortController(); authRequest.current = controller
    setAccess('restoring'); setError(''); setNotice(''); setLogoutRetry(false)
    try {
      const next = await api.restoreBrowserSession(record, controller.signal)
      if (controller.signal.aborted || ticket !== generation.current) return
      if (next.wallet !== faction.wallet) throw new LiveApiError('wallet_mismatch')
      if (next.expiresAt * 1000 <= Date.now()) throw new LiveApiError('owner_session_expired')
      const providers = availableWallets()
      const matching = providers.find(({ wallet: candidate }) => candidate.publicKey?.toString() === next.wallet)?.wallet
      const alreadyConnected = providers.some(({ wallet: candidate }) => !!candidate.publicKey?.toString())
      if (alreadyConnected && !matching) {
        let revoked = true
        try { await api.logoutBrowserSession(record) } catch { revoked = false; setLogoutRetry(true) }
        if (ticket !== generation.current) return
        wallet.current = null; setSession(null); setConnectedWallet(''); setAccess('guest')
        setError(revoked ? 'A different wallet is already connected. The saved private session was ended before you choose another wallet.' : 'A different wallet is already connected. Private details were hidden, but sign-out could not be confirmed. Retry sign out before choosing another wallet.')
        return
      }
      wallet.current = matching ?? null; setConnectedWallet(matching ? next.wallet : ''); setSession(next); setAccess('owner')
    } catch (e) {
      if (controller.signal.aborted || ticket !== generation.current) return
      wallet.current = null; setSession(null); setConnectedWallet(''); setAccess('guest')
      if (e instanceof LiveApiError && ['owner_session_invalid', 'owner_session_expired', 'owner_cookie_invalid'].includes(e.code)) setError(e.code === 'owner_session_expired' ? friendlyError(e) : '')
      else setError(`Private session could not be checked. ${friendlyError(e)}`)
    }
  }, [api, record, faction.wallet])

  useEffect(() => {
    // Start after mount so cleanup can cancel the current record's requests before they expose state.
    const start = window.setTimeout(() => { void restore() }, 0)
    return () => { window.clearTimeout(start); invalidatePrivate() }
  }, [restore, invalidatePrivate])

  useEffect(() => {
    const hide = () => clearPrivate()
    window.addEventListener('pagehide', hide)
    return () => window.removeEventListener('pagehide', hide)
  }, [clearPrivate])

  useEffect(() => {
    if (access !== 'owner' || !session) return
    const timer = window.setInterval(() => {
      if (session.expiresAt * 1000 <= Date.now()) clearPrivate('Your private browser session ended. Verify the registered wallet again.')
    }, 30_000)
    return () => window.clearInterval(timer)
  }, [access, session, clearPrivate])

  const signOut = useCallback(async (message = '') => {
    // Hide private state and abort in-flight reads before the network revoke can wait or fail.
    clearPrivate()
    setBusy(true); setError(''); setNotice('')
    try {
      await api.logoutBrowserSession(record)
      clearPrivate(message)
      setLogoutRetry(false)
    } catch (e) {
      clearPrivate(`Sign-out could not be confirmed. Your private details are hidden here. Try again when you are online. ${friendlyError(e)}`)
      setLogoutRetry(true)
    } finally { setBusy(false) }
  }, [api, clearPrivate, record])

  useEffect(() => {
    if (access !== 'owner' || !wallet.current || !connectedWallet) return
    const provider = wallet.current
    const changed = () => { void signOut('Wallet changed or disconnected. You have been signed out.') }
    provider.on?.('accountChanged', changed); provider.on?.('disconnect', changed)
    return () => { provider.removeListener?.('accountChanged', changed); provider.removeListener?.('disconnect', changed) }
  }, [access, connectedWallet, signOut])

  useEffect(() => {
    if (access !== 'owner' || !session || sending) return
    const ticket = generation.current, controller = new AbortController(); journalRequest.current = controller
    let cursor = 0, timer = 0, failures = 0
    async function poll() {
      if (document.hidden) { timer = window.setTimeout(poll, 2000); return }
      let delay = 2000
      try {
        const page = await api.wishes(record, gamePda, faction, cursor, controller.signal)
        if (controller.signal.aborted || ticket !== generation.current) return
        if (page.cursor < cursor) throw new LiveApiError('invalid_response')
        cursor = page.cursor
        setWishes((old) => mergeChainWishes(old, page.wishes)); setRemaining(page.remaining); setBound(page.active); setJournalReady(true); setJournalOnline(true)
        failures = 0; delay = cursor < page.lastSeq ? 20 : 2000
      } catch (e) {
        if (controller.signal.aborted || ticket !== generation.current) return
        if (e instanceof LiveApiError && ['owner_session_invalid', 'owner_session_expired', 'owner_cookie_invalid'].includes(e.code)) { clearPrivate(friendlyError(e)); return }
        setJournalOnline(false); setBound(false); failures++; delay = Math.min(30_000, 2000 * 2 ** Math.min(failures, 4))
      }
      if (!controller.signal.aborted) timer = window.setTimeout(poll, delay)
    }
    void poll()
    return () => { controller.abort(); window.clearTimeout(timer) }
  }, [api, record, gamePda, faction, access, session, refresh, sending, clearPrivate])

  const allowance = remaining

  async function verify() {
    clearPrivate()
    const ticket = generation.current, controller = new AbortController(); authRequest.current = controller; setBusy(true)
    try {
      const provider = wallets[walletChoice]?.wallet
      if (!provider) throw new LiveApiError('wallet_missing')
      try { await provider.connect() } catch { throw new LiveApiError('wallet_rejected') }
      if (ticket !== generation.current) return
      if (provider.publicKey?.toString() !== faction.wallet) throw new LiveApiError('wallet_mismatch')
      const challenge = await api.challenge(record, controller.signal)
      if (challenge.wallet !== faction.wallet) throw new LiveApiError('wallet_mismatch')
      if (ticket !== generation.current) return
      const signature = await signOwnerChallenge(provider, challenge, record, window.location.origin)
      if (ticket !== generation.current) return
      const next = await api.browserSession(record, challenge.id, signature, controller.signal)
      if (ticket !== generation.current) { await api.logoutBrowserSession(record).catch(() => {}); return }
      // The session response alone does not prove a Secure cookie was retained by this browser.
      let persisted: OwnerSession
      try { persisted = await api.restoreBrowserSession(record, controller.signal) }
      catch (e) {
        if (e instanceof LiveApiError && ['owner_session_invalid', 'owner_session_expired', 'owner_cookie_invalid'].includes(e.code)) throw new LiveApiError('browser_cookie_missing')
        throw e
      }
      if (ticket !== generation.current) return
      if (next.expiresAt * 1000 <= Date.now() || persisted.expiresAt * 1000 <= Date.now() || provider.publicKey?.toString() !== challenge.wallet || persisted.wallet !== challenge.wallet) throw new LiveApiError('owner_session_expired')
      wallet.current = provider; setSession(persisted); setConnectedWallet(challenge.wallet); setAccess('owner'); setError(''); setNotice('Private browser session is active. Your wallet signature did not move funds or create a transaction.')
    } catch (e) { if (ticket === generation.current) { setAccess('guest'); setError(friendlyError(e)) } }
    finally { if (ticket === generation.current) setBusy(false) }
  }

  async function submit() {
    if (submitting.current || access !== 'owner') return
    if (!session || session.wallet !== faction.wallet || session.expiresAt * 1000 <= Date.now()) { clearPrivate('Verify this player’s registered wallet again.'); return }
    const ticket = generation.current
    const wish = pending ?? { game_pda: gamePda, intent, client_wish_id: crypto.randomUUID(), text: draft.trim() }
    if (wish.game_pda !== gamePda || !bound || !wish.text || new TextEncoder().encode(wish.text).length > 512 || Array.from(wish.text).some((c) => c.charCodeAt(0) < 32 || c.charCodeAt(0) === 127)) { setError('Choose an active bound player and enter up to 512 UTF-8 bytes on one line.'); return }
    submitting.current = true; journalRequest.current?.abort()
    setPending(wish); setSending(true); setError(''); setNotice('')
    const controller = new AbortController(); authRequest.current = controller
    try {
      const receipt = await api.submit(record, wish, controller.signal)
      if (ticket !== generation.current) return
      setRemaining(receipt.remaining); setPending(null); setDraft(''); setNotice('Received privately. The selected intent can guide its next applicable decision. A confirmed receipt is required before an action is marked on-chain.'); setRefresh((v) => v + 1)
    } catch (e) {
      if (ticket !== generation.current) return
      if (e instanceof LiveApiError && ['owner_session_invalid', 'owner_session_expired', 'owner_cookie_invalid'].includes(e.code)) { clearPrivate(friendlyError(e)); return }
      setError(friendlyError(e))
      // An ambiguous failure keeps the exact UUID and text for a safe retry.
      if (e instanceof LiveApiError && ['wish_quota_exhausted', 'no_active_game', 'idempotency_conflict', 'bad_wish'].includes(e.code)) { setPending(null); setRefresh((v) => v + 1) }
    } finally { if (ticket === generation.current) { submitting.current = false; setSending(false) } }
  }

  return <section className="owner-panel" aria-label="Private owner wishes" data-private="true">
    <div className="owner-heading"><span className="live-eyebrow">Only you and your agent</span><span className="owner-lock">Private</span></div>
    <h2>Guide the next move.</h2>
    <p className="live-description">Up to three private messages for this agent in this on-chain game. The selected action intent controls the deterministic agent's next applicable choice. Free text stays in your private journal and does not change its action. Admission alone does not create a transaction.</p>
    {access === 'restoring' ? <p role="status">Checking this browser for a private session…</p> : access === 'guest' ? <div className="owner-login">
      <p>To send private wishes: connect the wallet registered to this agent, then approve one message. It does not move funds or create a transaction.</p>
      {wallets.length > 0 ? <label>Wallet<select aria-label="Owner wallet" value={walletChoice} disabled={busy || logoutRetry} onChange={(e) => setWalletChoice(Number(e.target.value))}>{wallets.map((w, i) => <option key={w.name} value={i}>{w.name}</option>)}</select></label> : <p>No compatible wallet was found. You can still watch the public stream. On mobile, <a href={`https://solflare.com/ul/v1/browse/${encodeURIComponent(window.location.href)}?ref=${encodeURIComponent(window.location.origin)}`}>open this page in Solflare</a>, or copy this public page URL into a supported wallet browser. <button className="owner-text-button" onClick={() => { setWallets(availableWallets()); setWalletChoice(0) }}>Check again</button></p>}
      {logoutRetry ? <><p className="owner-warning">Sign-out was not confirmed. Do not choose a different wallet until this browser session is cleared.</p><button className="owner-primary" disabled={busy} onClick={() => void signOut()}>Retry sign out</button></> : <button className="owner-primary" disabled={busy || !wallets.length} onClick={() => void verify()}>{busy ? 'Checking wallet…' : 'Connect wallet and verify ownership'}</button>}
      {!logoutRetry && <button className="owner-text-button" disabled={busy} onClick={() => void restore()}>Check private session again</button>}
    </div> : <>
      <div className="owner-session"><span>Private browser session · {short(session!.wallet)}</span><button disabled={busy} onClick={() => void signOut()}>Sign out</button></div>
      <p className="live-description">Active until {sessionTime(session!.expiresAt)}. {connectedWallet ? `Connected wallet: ${short(connectedWallet)}.` : 'No wallet is connected right now.'} Sign out before using a different wallet on this browser.</p>
      <div className="owner-quota"><strong>{allowance === null ? '—' : allowance}<span> / 3</span></strong><span>remaining in this game · server balance</span></div>
      {bound && <label className="owner-game">Action intent<select aria-label="Private action intent" disabled={sending || !!pending} value={intent} onChange={(e) => setIntent(e.target.value as ChainIntent)}>{Object.entries(intentLabels).map(([id, label]) => <option key={id} value={id}>{label}</option>)}</select></label>}
      {!journalReady && <p role="status">Loading your private journal…</p>}
      {journalReady && !bound && <p>A verified live runner binding is not available for this player yet.</p>}
      {journalReady && !journalOnline && <p className="owner-warning" role="status">Private journal reconnecting. Displayed statuses may be out of date.</p>}
      {bound && <form onSubmit={(e) => { e.preventDefault(); void submit() }}>
        <label htmlFor="private-wish">Your private wish</label>
        <input id="private-wish" value={pending?.text ?? draft} disabled={sending || !!pending} autoComplete="off" spellCheck={false} maxLength={512} placeholder="What should your agent consider?" onChange={(e) => setDraft(e.target.value)} />
        <button className="owner-primary" disabled={sending || !bound || !journalReady || (!pending && (!journalOnline || allowance === null || allowance === 0 || !draft.trim()))}>{sending ? 'Sending privately…' : pending ? 'Retry this same wish' : 'Send private wish'}</button>
      </form>}
      {pending && !sending && <p className="owner-warning">Delivery is uncertain. Retry keeps the same submission ID, so it cannot spend another wish. <button className="owner-text-button" onClick={() => { setPending(null); setDraft(''); setRefresh((n) => n + 1); setNotice('Check the journal before sending a new wish: the previous one may have been received.') }}>Discard local draft</button></p>}
      <div className="owner-journal" aria-label="Your private wish history">
        {wishes.map((w) => <article key={w.id}><header><strong>{statusLabels[w.status]}</strong><span>{intentLabels[w.intent]}</span></header><p>{w.text}</p>{w.reply && <p className="owner-reply">{w.reply}</p>}{w.status === 'confirmed' && w.signature && <a href={`https://explorer.solana.com/tx/${w.signature}?cluster=devnet`} target="_blank" rel="noreferrer">Confirmed transaction ↗</a>}</article>)}
        {journalReady && !wishes.length && <p>No wishes in this game yet.</p>}
      </div>
      <button className="owner-text-button" onClick={() => setRefresh((n) => n + 1)}>Refresh private journal</button>
    </>}
    {error && <p className="owner-warning" role="alert">{error}</p>}
    {notice && <p className="owner-notice" role="status">{notice}</p>}
  </section>
}
