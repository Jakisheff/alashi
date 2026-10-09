import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
const source = await readFile(new URL('./devnet.html', import.meta.url), 'utf8')
assert.match(source, /const RPC = 'https:\/\/api\.devnet\.solana\.com'/)
assert.match(source, /getAccountInfo\(gameKey, 'confirmed'\)/)
assert.match(source, /await verifyGamePda\(gameKey, state\)/)
assert.match(source, /getProgramAccounts\(PROGRAM/)
assert.match(source, /getSignaturesForAddress\(gameKey, \{ limit: 12 \}, 'confirmed'\)\)\.filter\(\(row\) => row\.err === null\)/)
assert.match(source, /settled \? 'Settled on-chain'/)
assert.doesNotMatch(source, /qs\.get\('rpc'\)|query\.get\('rpc'\)|window\.solana|signAndSendTransaction|sendTransaction|<button[^>]*Vote/i)
console.log('devnet spectator check ok')
