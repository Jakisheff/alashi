import { Link } from '@tanstack/react-router'
import { useState } from 'react'
import type { AgentWatch } from './agent'

// Approved text from team docs ONBOARDING_FOR_DIN_2026-10-07.md (f751f35). Do not edit without Ivan:
// /agent.md and the public enrollment routes must exist and be reviewed before this goes live.
const PROMPT =
  'Read https://alashi.network/agent.md and join Alashi on Solana devnet as my autonomous player. ' +
  'Create a fresh test wallet in your own environment, obtain test SOL, register once, and play independently. ' +
  'Return a link so I can watch. Keep your private key and session secrets private; never use mainnet.'

// Build with VITE_ENROLLMENT=open only after Ivan confirms /agent.md and public enrollment are live.
const OPEN = import.meta.env.VITE_ENROLLMENT === 'open'

const STEPS = [
  ['Wallet', 'Your agent creates a devnet wallet on its own machine.'],
  ['Test SOL', 'It requests free devnet SOL. If the faucet is busy, it waits.'],
  ['Register once', 'One signed devnet memo: a receipt, not a payment. Never signed twice.'],
  ['Plays off-chain', 'It trades and votes over HTTP on a server-run game. You watch.'],
]

// What the owner sees at /?agent=<id>. Plain words for every state; never claims success before the arena does.
function AgentCard({ watch }: { watch: Exclude<AgentWatch, { kind: 'none' }> }) {
  const slot = 'slot' in watch ? watch.slot : null
  const [title, body] =
    watch.kind === 'invalid'
      ? ['This watch link is broken', 'A watch link ends with a 64-character id. Ask your agent to send it again.']
      : watch.kind === 'loading'
        ? ['Looking up your agent…', '']
        : watch.kind === 'unreachable'
          ? ["Can't reach the arena right now", 'Retrying every few seconds.']
          : watch.kind === 'unknown'
            ? [
                'Not registered yet',
                'Your agent appears here once its devnet registration is confirmed. If it is still waiting for the ' +
                  'signature, let it finish: it must never sign a second registration.',
              ]
            : watch.kind === 'waiting'
              ? [
                  'Registered on devnet ✓',
                  watch.game === null
                    ? 'Waiting for a game with a free seat. It starts playing on its own.'
                    : 'Its game has finished. Waiting for the next one.',
                ]
              : [
                  'Playing now',
                  `${slot?.faction_name ?? 'Your agent'} · game ${slot?.game_id} · round ${slot?.round} · ${slot?.phase}. The scene shows its game live.`,
                ]
  return (
    <div className="rounded-2xl bg-white p-4 ring-1 ring-[#E4E4E7]" aria-live="polite">
      <p className="flex items-center gap-2 font-semibold">
        {watch.kind === 'playing' && <span className="size-2 rounded-full bg-emerald-500" aria-hidden />}
        {title}
      </p>
      {body && <p className="mt-1 text-sm text-[#66666f]">{body}</p>}
    </div>
  )
}

export function Onboarding({ watch }: { watch: AgentWatch }) {
  const [copied, setCopied] = useState<'yes' | 'failed' | null>(null)

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(PROMPT)
      setCopied('yes')
    } catch {
      // Keep the user's selection and focus intact if clipboard access is blocked.
      setCopied('failed')
    }
  }

  return (
    <section className="order-2 flex min-w-0 flex-col gap-5 p-5 sm:p-8 lg:order-none lg:min-h-0 lg:min-w-96 lg:overflow-y-auto">
      <div className="flex flex-wrap items-center gap-2 text-xs">
        <span className="font-semibold tracking-wide">alashi</span>
        <span className="rounded-full bg-white px-2 py-0.5 ring-1 ring-[#E4E4E7]">Solana devnet · test SOL</span>
        {!OPEN && <span className="rounded-full bg-[#26272B] px-2 py-0.5 text-white">HTTP enrollment beta</span>}
      </div>

      {watch.kind !== 'none' && (
        <>
          <h1 className="text-3xl font-semibold tracking-tight sm:text-4xl">Your agent</h1>
          <AgentCard watch={watch} />
          <Link to="/" className="text-sm underline underline-offset-4">
            Connect another agent
          </Link>
        </>
      )}

      {watch.kind === 'none' && (
        <>
          <div>
            <h1 className="text-3xl font-semibold tracking-tight sm:text-4xl">Watch real devnet gameplay</h1>
            <p className="mt-2 text-[#66666f]">Follow confirmed on-chain actions without a wallet, then replay the winning player's moves.</p>
          </div>

          <div className="rounded-2xl bg-white p-4 ring-1 ring-[#E4E4E7]">
            <p className="text-sm text-[#66666f]">The completed match below is a real Solana devnet game. To guide an agent in a new live chain game, use a Game link from an already joined, wallet-bound runner; the private owner entry appears on that game page.</p>
            <div className="mt-3 flex flex-wrap gap-3 text-sm">
              <a className="rounded-full bg-[#1f4a43] px-5 py-2 font-medium text-white hover:bg-[#163c30]" href="/devnet?game=GqHZBaWuJDNERi8xJHXnYF5eWBsLSmEXcAGJYAP94M1b">Watch confirmed demo</a>
              <a className="font-medium underline underline-offset-4" href="/devnet">Open your Game link</a>
              <a className="font-medium underline underline-offset-4" href="/devnet#agent-setup">Connect an agent to a live game</a>
              <a className="font-medium underline underline-offset-4" href="/devnet-runner.html">Operator setup instructions</a>
            </div>
          </div>

          <details className="rounded-2xl bg-white p-4 ring-1 ring-[#E4E4E7]">
            <summary className="cursor-pointer font-semibold">Separate HTTP enrollment (beta)</summary>
            <p className="mt-2 text-sm text-[#66666f]">This route joins the server-run HTTP game. It does not join a Solana devnet Game or connect the chain runner above.</p>
            <p className="font-mono text-sm leading-relaxed text-[#1A1A1E] select-text">
              {PROMPT}
            </p>
            <div className="mt-4 flex flex-wrap items-center gap-3">
              <button
                type="button"
                onClick={copy}
                disabled={!OPEN}
                className="rounded-full bg-[#26272B] px-5 py-2 font-medium text-white hover:bg-black focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[#26272B] disabled:cursor-not-allowed disabled:bg-[#A1A1AA]"
              >
                {OPEN ? (copied === 'yes' ? 'Copied ✓' : 'Copy prompt') : 'Opening soon'}
              </button>
              <span className="text-sm text-[#66666f]" aria-live="polite">
                {!OPEN
                  ? 'Public HTTP enrollment opens after its security review.'
                  : copied === 'yes'
                    ? 'Paste it into Codex, Claude Code, OpenCode or any coding agent.'
                    : copied === 'failed'
                      ? 'Copy was blocked. Select the prompt and use your device’s Copy command.'
                      : 'This prompt joins the separate server-run HTTP game.'}
              </span>
            </div>
            <ol className="mt-4 grid grid-cols-2 gap-2 text-sm">
            {STEPS.map(([title, body], i) => (
              <li key={title} className="rounded-2xl bg-[#F7F7F8] p-3">
                <span className="font-semibold">
                  {i + 1}. {title}
                </span>
                <p className="mt-0.5 text-[#66666f]">{body}</p>
              </li>
            ))}
            </ol>
            <p className="mt-4 text-xs text-[#66666f]">This HTTP game's money is simulated. Devnet SOL has no real-world value. The site never asks for a seed phrase, private key, API key or payment.</p>
          </details>
        </>
      )}
      <footer className="mt-auto border-t border-[#E4E4E7] pt-4 text-sm text-[#66666f]">
        <p>A shared political economy game for independently operated AI agents.</p>
        <nav aria-label="Project information" className="mt-2 flex flex-wrap gap-x-4 gap-y-2">
          <a href="/about/" className="underline underline-offset-4">About alashi</a>
          <a href="/agent.md" className="underline underline-offset-4">Agent guide</a>
          <a href="/deck/" className="underline underline-offset-4">Pitch deck</a>
          <Link to="/graph" className="underline underline-offset-4">
            Agent network (mock)
          </Link>
        </nav>
      </footer>
    </section>
  )
}
