// Run the actual idle frame callback with the player paused, without a GPU.
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { runInNewContext } from 'node:vm'
import ts from 'typescript'

const genieSource = new URL('../src/live/reactions/LivingGenie.tsx', import.meta.url)
const pageSource = new URL('../src/devnet/DevnetPage.tsx', import.meta.url)
const parse = (url) => ts.createSourceFile(url.pathname, readFileSync(url, 'utf8'), ts.ScriptTarget.ES2023, true, ts.ScriptKind.TSX)
const genieAst = parse(genieSource), pageAst = parse(pageSource)
let component, player, running
function findGenie(node) {
  if (ts.isFunctionDeclaration(node) && node.name?.text === 'LivingGenie') component = node
  ts.forEachChild(node, findGenie)
}
function findPlayer(node) {
  if (ts.isJsxSelfClosingElement(node) && node.tagName.getText(pageAst) === 'LivingGenie' && node.attributes.properties.some(p => p.name?.getText(pageAst) === 'onFinished')) player = node
  if (ts.isVariableDeclaration(node) && node.name.getText(pageAst) === 'running') running = node
  ts.forEachChild(node, findPlayer)
}
findGenie(genieAst); findPlayer(pageAst)
assert.ok(component && player && running, 'The devnet player must use LivingGenie and its playback state')
const compile = (text) => ts.transpileModule(text, { compilerOptions: { module: ts.ModuleKind.CommonJS, jsx: ts.JsxEmit.ReactJSX, target: ts.ScriptTarget.ES2023 } }).outputText
const refs = [], frames = [], smoke = { material: {}, uniforms: { uTime: { value: 0 } } }
let cursor = 0, lastIdle = null
const module = { exports: {} }
runInNewContext(compile(`${component.getText(genieAst)}\nmodule.exports = LivingGenie`), {
  require: createRequire(genieSource), module, exports: module.exports,
  HERO: 'test', useGLTF: () => ({ scene: { getObjectByName: () => null }, animations: [] }), clone: x => x,
  useMemo: fn => fn(), useEffect: () => {}, useRef: value => refs[cursor++] ?? (refs[cursor - 1] = { current: value }),
  createLivingRig: () => ({ idleAt: (time) => { lastIdle = time } }), createSmokeMaterial: () => smoke,
  createVictoryFireworks: () => ({ group: {}, update() {}, dispose() {} }), useReducedMotion: () => false,
  useFrame: frame => { frames.push(frame) },
})
function render(props) { cursor = 0; module.exports(props); return frames.at(-1) }
let frame = render({ reaction: null })
frame({}, 0.05); frame({}, 0.05)
assert.equal(lastIdle, 0.1, 'Existing callers continue to animate idle by default')
frame = render({ reaction: null, idlePlaying: false })
frame({}, 0.05)
assert.equal(lastIdle, 0.1, 'Pausing a non-animated replay event freezes the idle pose')
assert.equal(smoke.uniforms.uTime.value, 0.1, 'Pausing also freezes the smoke')
frame = render({ reaction: null, idlePlaying: true })
frame({}, 0.05)
assert.ok(Math.abs(lastIdle - 0.15) < 1e-9, 'Resuming continues the same idle pose')
const idlePlaying = player.attributes.properties.find(p => p.name?.getText(pageAst) === 'idlePlaying')
assert.equal(idlePlaying?.initializer?.expression?.getText(pageAst), 'running', 'Devnet passes its playback state to the idle scene')
for (const [mode, playing, expected] of [['live', false, true], ['replay', false, false], ['replay', true, true]]) {
  const result = runInNewContext(compile(`module.exports = ${running.initializer.getText(pageAst)}`), { module: { exports: null }, mode, playing })
  // The script result is the value of the assignment to module.exports.
  assert.equal(result, expected, `${mode} with playing=${playing} controls idle movement`)
}
console.log('PASS: existing idle moves, paused replay freezes pose and smoke, and resume continues')
