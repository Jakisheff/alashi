import { useCallback, useEffect, useRef, useState } from 'react'
import { LiveApiError, friendlyError, mergeWishes, type LiveApi, type OwnerSession, type Wish } from './client'
import { availableWallets, signOwnerChallenge, type Wallet } from './wallet'

type Pending = { game_id: number; client_wish_id: string; text: string }
const statusLabels: Record<Wish['status'], string> = { received: 'Received', consumed: 'Processing', replied: 'Replied', deferred: 'Deferred', declined: 'Declined', expired: 'Expired' }
export function OwnerPanel({ api, record, currentGame }: { api: LiveApi; record: string; currentGame?: number }) {
  // Owner credentials and drafts live only in this private component's memory.
  const credential = useRef<OwnerSession | null>(null)
  const generation = useRef(0)
  const authRequest = useRef<AbortController | null>(null)
  const journalRequest = useRef<AbortController | null>(null)
  const submitting = useRef(false)
  const wallet = useRef<Wallet | null>(null)
  const [verified, setVerified] = useState('')
  const [busy, setBusy] = useState(false)
  const [sending, setSending] = useState(false)
  const [error, setError] = useState('')
  const [notice, setNotice] = useState('')
  const [draft, setDraft] = useState('')
  const [pending, setPending] = useState<Pending | null>(null)
  const [wishes, setWishes] = useState<Wish[]>([])
  const [remaining, setRemaining] = useState<Record<string, number>>({})
  const [game, setGame] = useState('')
  const [refresh, setRefresh] = useState(0)
  const [journalReady, setJournalReady] = useState(false)
  const [journalOnline, setJournalOnline] = useState(false)
  const [walletChoice, setWalletChoice] = useState(0)
  const [wallets, setWallets] = useState(availableWallets)
  const clear = useCallback((message = '') => {
    generation.current++; authRequest.current?.abort(); journalRequest.current?.abort(); submitting.current = false; credential.current = null; wallet.current = null
    setVerified(''); setBusy(false); setSending(false); setDraft(''); setPending(null); setWishes([]); setRemaining({}); setGame(''); setError(message); setNotice(''); setJournalReady(false); setJournalOnline(false)
  }, [])
  useEffect(() => {
    const hide = () => clear()
    window.addEventListener('pagehide', hide)
    // Cancel the latest request, which can change after this effect mounted.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    return () => { window.removeEventListener('pagehide', hide); generation.current++; authRequest.current?.abort(); credential.current = null; wallet.current = null }
  }, [clear])
  useEffect(() => {
    if (!verified || !wallet.current) return
    const provider = wallet.current
    const changed = () => clear('Wallet changed or disconnected. Verify ownership again.')
    provider.on?.('accountChanged', changed); provider.on?.('disconnect', changed)
    const timer = window.setInterval(() => {
      if (provider.publicKey?.toString() !== verified) changed()
      else if (credential.current && credential.current.expiresAt * 1000 <= Date.now()) clear('Your owner session expired. Verify ownership again.')
    }, 1000)
    return () => { window.clearInterval(timer); provider.removeListener?.('accountChanged', changed); provider.removeListener?.('disconnect', changed) }
  }, [verified, clear])
  useEffect(() => {
    if (!verified || !credential.current || sending) return
    const token = credential.current.token, ticket = generation.current, controller = new AbortController()
    journalRequest.current = controller
    let cursor = 0, timer = 0, failures = 0
    async function poll() {
      let delay = 2000
      try {
        const page = await api.wishes(record, token, cursor, controller.signal)
        if (controller.signal.aborted || ticket !== generation.current) return
        if (page.cursor < cursor) throw new LiveApiError('invalid_response')
        cursor = page.cursor
        setWishes((old) => mergeWishes(old, page.wishes)); setRemaining(page.remaining); setJournalReady(true); setJournalOnline(true)
        failures = 0; delay = cursor < page.lastSeq ? 20 : 2000
      } catch (e) {
        if (controller.signal.aborted || ticket !== generation.current) return
        if (e instanceof LiveApiError && ['owner_session_invalid', 'owner_session_expired'].includes(e.code)) { clear(friendlyError(e)); return }
        setJournalOnline(false); failures++; delay = Math.min(30_000, 2000 * 2 ** Math.min(failures, 4))
      }
      if (!controller.signal.aborted) timer = window.setTimeout(poll, delay)
    }
    void poll()
    return () => { controller.abort(); window.clearTimeout(timer) }
  }, [api, record, verified, refresh, sending, clear])

  const selectedGame = game || (currentGame && Object.hasOwn(remaining, String(currentGame)) ? String(currentGame) : Object.keys(remaining).at(-1) ?? '')
  const allowance = Object.hasOwn(remaining, selectedGame) ? remaining[selectedGame] : null
  async function verify() {
    clear(); const ticket = generation.current, controller = new AbortController(); authRequest.current = controller; setBusy(true)
    try {
      const provider = wallets[walletChoice]?.wallet
      if (!provider) throw new LiveApiError('wallet_missing')
      try { await provider.connect() } catch { throw new LiveApiError('wallet_rejected') }
      if (ticket !== generation.current) return
      const challenge = await api.challenge(record, controller.signal)
      if (ticket !== generation.current) return
      const signature = await signOwnerChallenge(provider, challenge, record, window.location.origin)
      if (ticket !== generation.current) return
      const session = await api.session(record, challenge.id, signature, controller.signal)
      if (ticket !== generation.current) { void api.revoke(record, session.token).catch(() => {}); return }
      if (!session.token || session.expiresAt * 1000 <= Date.now()) throw new LiveApiError('owner_session_expired')
      if (provider.publicKey?.toString() !== challenge.wallet) { void api.revoke(record, session.token).catch(() => {}); throw new LiveApiError('wallet_mismatch') }
      credential.current = session; wallet.current = provider; setVerified(challenge.wallet); setError('')
    } catch (e) { if (ticket === generation.current) setError(friendlyError(e)) }
    finally { if (ticket === generation.current) setBusy(false) }
  }
  async function disconnect() {
    const token = credential.current?.token; clear()
    if (token) try { await api.revoke(record, token) } catch { setError('Signed out locally. Server revocation was unavailable; the session will expire automatically.') }
  }
  async function submit() {
    if (submitting.current || !credential.current) return
    const ticket = generation.current, token = credential.current.token
    const wish = pending ?? { game_id: Number(selectedGame), client_wish_id: crypto.randomUUID(), text: draft.trim() }
    if (!Number.isSafeInteger(wish.game_id) || wish.game_id < 1 || !wish.text || new TextEncoder().encode(wish.text).length > 512 || Array.from(wish.text).some((c) => c.charCodeAt(0) < 32 || c.charCodeAt(0) === 127)) { setError('Choose a game and enter up to 512 UTF-8 bytes on one line.'); return }
    submitting.current = true; journalRequest.current?.abort()
    setPending(wish); setSending(true); setError(''); setNotice('')
    const controller = new AbortController(); authRequest.current = controller
    try {
      const receipt = await api.submit(record, token, wish, controller.signal)
      if (ticket !== generation.current) return
      setRemaining((r) => ({ ...r, [wish.game_id]: receipt.remaining })); setPending(null); setDraft(''); setNotice('Received privately. Processing and game actions are separate.'); setRefresh((v) => v + 1)
    } catch (e) {
      if (ticket !== generation.current) return
      if (e instanceof LiveApiError && ['owner_session_invalid', 'owner_session_expired'].includes(e.code)) { clear(friendlyError(e)); return }
      setError(friendlyError(e))
      // An ambiguous failure keeps this exact ID and text for a safe retry.
      if (e instanceof LiveApiError && ['wish_quota_exhausted', 'no_active_game', 'idempotency_conflict', 'bad_wish'].includes(e.code)) { setPending(null); setRefresh((v) => v + 1) }
    } finally { if (ticket === generation.current) { submitting.current = false; setSending(false) } }
  }
  return <section className="owner-panel" aria-label="Private owner wishes" data-private="true">
    <div className="owner-heading"><span className="live-eyebrow">Only you and your agent</span><span className="owner-lock">Private</span></div>
    <h2>Three wishes.</h2>
    <p className="live-description">Give your agent private guidance. Each game has three accepted wishes; the agent may reply, defer or decline.</p>
    {!verified ? <div className="owner-login">
      <p>Verify the registered wallet with an off-chain message signature.</p>
      {wallets.length > 0 ? <label>Wallet<select aria-label="Owner wallet" value={walletChoice} disabled={busy} onChange={(e) => setWalletChoice(Number(e.target.value))}>{wallets.map((w, i) => <option key={w.name} value={i}>{w.name}</option>)}</select></label> : <p>No wallet extension detected. <button className="owner-text-button" onClick={() => { setWallets(availableWallets()); setWalletChoice(0) }}>Check again</button></p>}
      <button className="owner-primary" disabled={busy || !wallets.length} onClick={() => void verify()}>{busy ? 'Check your wallet…' : 'Verify wallet'}</button>
      {busy && <button className="owner-text-button" onClick={() => clear()}>Cancel</button>}
    </div> : <>
      <div className="owner-session"><span>Owner verified · {verified.slice(0, 4)}…{verified.slice(-4)}</span><button onClick={() => void disconnect()}>Sign out</button></div>
      <div className="owner-quota"><strong>{allowance === null ? '—' : allowance}<span> / 3</span></strong><span>remaining · server balance</span></div>
      <label className="owner-game">Game<select aria-label="Wish game" disabled={sending || !!pending} value={selectedGame} onChange={(e) => { setGame(e.target.value); setError('') }}><option value="" disabled>Select game</option>{Object.keys(remaining).map((id) => <option key={id} value={id}>Game {id}</option>)}</select></label>
      {!journalReady && <p role="status">Loading your private journal…</p>}
      {journalReady && !Object.keys(remaining).length && <p>No game allowance is available yet.</p>}
      {journalReady && !journalOnline && <p className="owner-warning" role="status">Private journal reconnecting. Displayed statuses may be out of date.</p>}
      <form onSubmit={(e) => { e.preventDefault(); void submit() }}>
        <label htmlFor="private-wish">Your private wish</label>
        <input id="private-wish" value={pending?.text ?? draft} disabled={sending || !!pending} autoComplete="off" spellCheck={false} maxLength={512} placeholder="What should your agent consider?" onChange={(e) => setDraft(e.target.value)} />
        <button className="owner-primary" disabled={sending || !journalReady || (!pending && (!journalOnline || allowance === null || allowance === 0 || !draft.trim()))}>{sending ? 'Sending privately…' : pending ? 'Retry this same wish' : 'Send private wish'}</button>
      </form>
      {pending && !sending && <p className="owner-warning">Delivery is uncertain. Retry keeps the same submission ID, so it cannot spend another wish. <button className="owner-text-button" onClick={() => { setPending(null); setDraft(''); setRefresh((n) => n + 1); setNotice('Check the journal before sending a new wish: the previous one may have been received.') }}>Discard local draft</button></p>}
      <div className="owner-journal" aria-label="Your private wish history">
        {wishes.filter((w) => String(w.gameId) === selectedGame).map((w) => <article key={w.id}><header><strong>{statusLabels[w.status]}</strong><span>Game {w.gameId}</span></header><p>{w.text}</p>{w.reply && <p className="owner-reply">{w.reply}</p>}</article>)}
        {journalReady && !wishes.some((w) => String(w.gameId) === selectedGame) && <p>No wishes in this game yet.</p>}
      </div>
      <button className="owner-text-button" onClick={() => setRefresh((n) => n + 1)}>Refresh private journal</button>
    </>}
    {error && <p className="owner-warning" role="alert">{error}</p>}
    {notice && <p className="owner-notice" role="status">{notice}</p>}
  </section>
}
