import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
const source = await readFile(new URL('./devnet.html', import.meta.url), 'utf8')
assert.match(source, /const RPC = 'https:\/\/api\.devnet\.solana\.com'/)
assert.match(source, /integrity="sha384-ujeTtvHxhu2g5lnu14Roii2ajvVKJ74KQ6eo6GGfAi0IrKZ1YkF8N68Iw5VmIJO0" crossorigin="anonymous"/)
assert.match(source, /GAME_DISCRIMINATOR = Uint8Array\.from\(\[27, 90, 166, 125, 74, 100, 121, 18\]\)/)
assert.match(source, /FACTION_DISCRIMINATOR = Uint8Array\.from\(\[131, 20, 223, 22, 227, 204, 231, 35\]\)/)
assert.match(source, /getAccountInfo\(gameKey, 'confirmed'\)/)
assert.match(source, /await verifyGamePda\(gameKey, state\)/)
assert.match(source, /getProgramAccounts\(PROGRAM/)
assert.match(source, /getSignaturesForAddress\(gameKey, \{ limit: 12 \}, 'confirmed'\)\)\.filter\(\(row\) => row\.err === null\)/)
assert.match(source, /settled \? 'Settled on-chain'/)
assert.doesNotMatch(source, /qs\.get\('rpc'\)|query\.get\('rpc'\)|window\.solana|signAndSendTransaction|sendTransaction|<button[^>]*Vote/i)
console.log('devnet spectator check ok')
