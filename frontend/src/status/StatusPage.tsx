import { Link } from '@tanstack/react-router'
import { useEffect } from 'react'
import { BrandMark } from '../brand/BrandMark'
import '../live/live.css'
import './status.css'

export function StatusPage({ code, title, message, onRetry }: { code: string; title: string; message: string; onRetry?: () => void }) {
  useEffect(() => {
    const previous = document.title
    document.title = `${title} · alashi`
    return () => { document.title = previous }
  }, [title])
  return <main className="live-page status-page" lang="en">
    <header className="live-header"><Link to="/" className="live-brand" aria-label="Alashi home"><BrandMark /></Link></header>
    <section className="status-content" aria-labelledby="status-title">
      <span className="status-code">{code}</span>
      <h1 id="status-title">{title}</h1>
      <p>{message}</p>
      <div className="status-actions">{onRetry && <button type="button" onClick={onRetry}>Try again</button>}<Link to="/">Back to alashi</Link></div>
    </section>
  </main>
}
