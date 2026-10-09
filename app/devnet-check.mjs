import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import vm from 'node:vm'
const source = await readFile(new URL('./devnet.html', import.meta.url), 'utf8')
assert.match(source, /const RPC = 'https:\/\/api\.devnet\.solana\.com'/)
assert.match(source, /integrity="sha384-ujeTtvHxhu2g5lnu14Roii2ajvVKJ74KQ6eo6GGfAi0IrKZ1YkF8N68Iw5VmIJO0" crossorigin="anonymous"/)
assert.match(source, /GAME_DISCRIMINATOR = Uint8Array\.from\(\[27, 90, 166, 125, 74, 100, 121, 18\]\)/)
assert.match(source, /FACTION_DISCRIMINATOR = Uint8Array\.from\(\[131, 20, 223, 22, 227, 204, 231, 35\]\)/)
assert.match(source, /getAccountInfo\(key, 'confirmed'\)/)
assert.match(source, /await verifyGamePda\(key, state\)/)
assert.match(source, /getProgramAccounts\(PROGRAM/)
assert.match(source, /getSignaturesForAddress\(key, \{ limit: 12 \}, 'confirmed'\)\)\.filter\(\(row\) => row\.err === null\)/)
assert.match(source, /settled \? 'Settled on-chain'/)
assert.doesNotMatch(source, /qs\.get\('rpc'\)|query\.get\('rpc'\)|window\.solana|signAndSendTransaction|sendTransaction|<button[^>]*Vote/i)

const script = source.match(/<script>\n([\s\S]*?)\n<\/script>/)?.[1]
assert.ok(script, 'viewer script is present')
const element = () => ({ addEventListener() {} })
const context = {
  DataView, TextDecoder, Uint8Array, URLSearchParams,
  document: { getElementById: element, addEventListener() {} },
  history: { replaceState() {} },
  location: { pathname: '/devnet', search: '' },
  setTimeout,
  solanaWeb3: {
    PublicKey: class { constructor(value) { this.value = value } },
    Connection: class { constructor() {} },
  },
  window: { setTimeout: () => 1, clearTimeout() {}, addEventListener() {} },
}
vm.createContext(context)
vm.runInContext(script, context)
const faction = new Uint8Array(97)
faction.set([131, 20, 223, 22, 227, 204, 231, 35])
const view = new DataView(faction.buffer)
view.setUint32(72, 4, true)
faction.set(new TextEncoder().encode('Aibot'), 76)
view.setBigUint64(80, 73_000_000n, true)
view.setUint16(88, 2, true)
view.setUint16(90, 3, true)
view.setUint16(92, 19, true) // acted_stamp
view.setUint16(94, 51, true) // voted_stamp: must not be rendered as the vote
faction[96] = 1 // VoteChoice::No
assert.equal(context.parseFaction(faction).vote, 1)
assert.throws(() => context.parseFaction(faction.subarray(0, 96)), /Faction name is invalid/)
console.log('devnet spectator check ok')
