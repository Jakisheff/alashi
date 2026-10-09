/** The caller supplies display copy; this component never reads private agent state. */
export function ThoughtBubble({ text, preview = false }: { text: string; preview?: boolean }) {
  if (!text.trim()) return null
  return <div className="live-thought-bubble" role="status" aria-label={preview ? 'Demo thought' : 'Shared thought'}>
    <svg viewBox="0 0 240 96" preserveAspectRatio="none" aria-hidden="true">
      <path d="M31 75C6 75 0 48 18 34C14 15 41 4 60 16C72-2 106-1 119 12C139-2 174 4 181 20C210 10 236 25 226 44C248 61 226 85 204 79C189 99 152 93 141 82C121 98 88 94 78 82C57 95 37 91 31 75Z" />
    </svg>
    <p>{text}</p>
    <span className="live-thought-trail" aria-hidden="true"><i /><i /></span>
  </div>
}
