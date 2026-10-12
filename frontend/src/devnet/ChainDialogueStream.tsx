import type { ChainSnapshot } from './client'
import { dialogueView, type DialogueEntry } from './dialogueClient'
import './chainDialogueStream.css'
export function ChainDialogueStream({ entries, snapshot, replayEvent, loaded, error, retry }: {
  entries: readonly DialogueEntry[]; snapshot: ChainSnapshot; replayEvent: string | null; loaded: boolean; error: string; retry: () => void
}) {
  const view = dialogueView(entries, snapshot, replayEvent)
  const names = new Map(snapshot.factions.map((f) => [f.pda, f.name]))
  const name = (pda: string) => names.get(pda) ?? `${pda.slice(0, 6)}…${pda.slice(-4)}`
  const rows = (items: readonly DialogueEntry[]) => <ol>{items.map((e) => {
    const reply = e.reply_to_message_id ? entries.find((m) => m.message_id === e.reply_to_message_id) : null
    return <li key={e.message_id} id={`dialogue-${e.message_id}`}>
      <div className="chain-dialogue-author"><strong>{name(e.author_faction_pda)}</strong><span> → {name(e.to_faction_pda)}</span></div>
      {e.reply_to_message_id && <small>{reply ? `Reply to ${name(reply.author_faction_pda)}` : 'Reply to an earlier recorded message'}</small>}
      <p>{e.text}</p>
      <small>Round {e.round} · {e.phase} · off-chain message</small>
      {e.related_offer_entry_id && <small> · Refers to a recorded trade offer</small>}
    </li>
  })}</ol>
  return <section className="chain-dialogue" aria-label="Agent dialogue">
    <h2>Agent dialogue</h2><p className="chain-dialogue-note">Recorded by the runners. Messages do not confirm a trade or move funds.</p>
    {error && <p role="status">{error} <button onClick={retry}>Retry dialogue</button></p>}
    {!loaded && !error && <p role="status">Loading dialogue…</p>}
    {view.timeline.length > 0 ? rows(view.timeline) : loaded && <p>No messages at this point in the game.</p>}
    {view.separate.length > 0 && <details><summary>Separate recorded history ({view.separate.length})</summary><p className="chain-dialogue-note">These messages have no available chain anchor and are shown separately from replay.</p>{rows(view.separate)}</details>}
  </section>
}
