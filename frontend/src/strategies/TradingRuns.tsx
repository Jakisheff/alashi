import { useEffect, useState } from 'react'
import { library, strategyRequest, cashDelta, type Run } from './client'

export function TradingRuns({ mode, game }: { mode: 'http' | 'solana'; game: string }) {
  const [runs, setRuns] = useState<Run[]>([]), [available, setAvailable] = useState<boolean | null>(null)
  useEffect(() => {
    const controller = new AbortController(); let timer = 0
    setRuns([]); setAvailable(null)
    async function poll() {
      try {
        const data = library(await strategyRequest('/strategy-library', undefined, controller.signal))
        if (!controller.signal.aborted) { setRuns(data.runs.filter(r => r.brief.trading?.mode === mode && r.brief.trading.game === game)); setAvailable(true) }
      } catch { if (!controller.signal.aborted) setAvailable(false) }
      if (!controller.signal.aborted) timer = window.setTimeout(poll, 2000)
    }
    void poll(); return () => { controller.abort(); clearTimeout(timer) }
  }, [mode, game])
  return <section className="trading-strategies"><h2>Strategies and model tokens</h2>
    {available === null && <p>Checking attached strategy receipts. Model token use is not yet known.</p>}
    {available === false && <p>Usage service unavailable. Last received receipts stay visible.</p>}
    {available && !runs.length && <p>No strategy execution receipts are attached to this game. Model token use is unknown.</p>}
    <div className="task-grid">{runs.map(r => <article key={r.id}><span>Owner {r.owner.slice(0, 8)} · round {r.brief.trading!.round}</span><strong>{r.known_tokens.toLocaleString('en-US')} {r.test_provider ? 'TEST tokens' : 'reported tokens'}</strong><p>{r.unknown_usage ? 'Additional usage UNKNOWN. Work stopped.' : `${r.reserved_tokens} tokens held for pending calls.`}</p>{r.result?.trade_receipt && <p>{r.result.trade_receipt.action}: {cashDelta(r.result.trade_receipt.cash_delta)} alashi, {r.result.trade_receipt.goods_delta > 0 ? '+' : ''}{r.result.trade_receipt.goods_delta} goods</p>}<a href={`/strategies?challenge=${r.challenge}`}>Tasks, receipts and reusable strategy</a></article>)}</div>
    {!!runs.length && <p>These counts cover the attached strategy runs. Other calls are unknown. Usage is owner-submitted; TEST values do not measure real model consumption.</p>}
    <a href="/strategies#strategy-owner">Use or improve a strategy</a>
  </section>
}
