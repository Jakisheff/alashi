import { LiveApiError, type Challenge } from './client.ts'

export type Wallet = {
  publicKey?: { toString(): string } | null
  connect(): Promise<unknown>
  signMessage(message: Uint8Array, encoding?: string): Promise<Uint8Array | { signature: Uint8Array }>
  on?(event: string, listener: () => void): void
  removeListener?(event: string, listener: () => void): void
}
type WalletWindow = Window & { phantom?: { solana?: Wallet }; solana?: Wallet; solflare?: Wallet }
export function availableWallets(host: Window = window): { name: string; wallet: Wallet }[] {
  const w = host as WalletWindow, wallets = [
    { name: 'Phantom', wallet: w.phantom?.solana },
    { name: 'Solflare', wallet: w.solflare },
    { name: 'Browser wallet', wallet: w.solana },
  ]
  const seen = new Set<Wallet>()
  return wallets.filter((entry): entry is { name: string; wallet: Wallet } => {
    if (!entry.wallet || typeof entry.wallet.connect !== 'function' || typeof entry.wallet.signMessage !== 'function' || seen.has(entry.wallet)) return false
    seen.add(entry.wallet); return true
  })
}
const alphabet = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz'
export function base58(bytes: Uint8Array) {
  let n = 0n
  for (const byte of bytes) n = n * 256n + BigInt(byte)
  let result = ''
  while (n) { result = alphabet[Number(n % 58n)] + result; n /= 58n }
  for (const byte of bytes) { if (byte !== 0) break; result = `1${result}` }
  return result
}
// Sign only the exact owner-auth envelope for the current origin and selected record.
export function validateChallenge(c: Challenge, record: string, wallet: string, origin: string, now = Date.now() / 1000) {
  const lines = c.message.split('\n')
  if (c.wallet !== wallet) throw new LiveApiError('wallet_mismatch')
  if (!/^[1-9A-HJ-NP-Za-km-z]{32,44}$/.test(wallet) || !/^[0-9a-f]{64}$/.test(c.id)
    || lines.length !== 8 || lines[0] !== 'alashi-owner-auth-v1' || lines[1] !== `origin:${origin}`
    || lines[2] !== `agent_record_id:${record}` || lines[3] !== `wallet:${wallet}` || !/^nonce:[0-9a-f]{64}$/.test(lines[4])
    || lines[5] !== `issued_at:${c.issuedAt}` || lines[6] !== `expires_at:${c.expiresAt}` || lines[7] !== ''
    || c.expiresAt <= now || c.issuedAt > now + 60 || c.expiresAt - c.issuedAt > 600 || c.expiresAt <= c.issuedAt) throw new LiveApiError('challenge_mismatch')
}
export async function signOwnerChallenge(wallet: Wallet, challenge: Challenge, record: string, origin: string) {
  const key = wallet.publicKey?.toString() ?? ''
  validateChallenge(challenge, record, key, origin)
  let signed: Uint8Array | { signature: Uint8Array }
  try { signed = await wallet.signMessage(new TextEncoder().encode(challenge.message), 'utf8') } catch { throw new LiveApiError('wallet_rejected') }
  if (wallet.publicKey?.toString() !== key) throw new LiveApiError('wallet_mismatch')
  const signature = signed instanceof Uint8Array ? signed : signed.signature
  if (!(signature instanceof Uint8Array) || signature.length !== 64) throw new LiveApiError('wallet_rejected')
  return base58(signature)
}
