import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { runInNewContext } from 'node:vm'
import ts from 'typescript'
import { speechFrame, speechPages } from '../src/genie/screenTimeline.ts'
import { actionStageAt } from '../src/live/actions/definitions.ts'
import { PURCHASE_CONTACT } from '../src/live/market/purchase.ts'

const source = new URL('../src/genie/ScreenText.tsx', import.meta.url)
const require = createRequire(source)
const compiled = ts.transpileModule(readFileSync(source, 'utf8'), {
  compilerOptions: { module: ts.ModuleKind.CommonJS, jsx: ts.JsxEmit.ReactJSX, target: ts.ScriptTarget.ES2023 },
}).outputText.replaceAll('import.meta.env.BASE_URL', JSON.stringify('/'))

function screen(props = {}) {
  const hooks = [], speech = { speech: 'The parcel stays with me until the next move.', speechId: 1, clip: 'idle' }
  let index = 0, frame
  const react = {
    useMemo: (sample) => sample(),
    useRef: (value) => { const i = index++; return hooks[i] ?? (hooks[i] = { current: value }) },
    useState: (value) => { const i = index++; hooks[i] ??= { value }; return [hooks[i].value, (next) => { hooks[i].value = next }] },
  }
  const module = { exports: {} }
  const mocked = (name) => name === 'react' ? react
    : name === '@react-three/fiber' ? { useFrame: (callback) => { frame = callback } }
    : name === '@react-three/drei' ? { Text: 'Text' }
    : name === '../store' ? { useScene: (select) => select(speech) } : require(name)
  runInNewContext(compiled, { require: mocked, module, exports: module.exports })
  const render = () => { index = 0; return module.exports.ScreenText(props).props.children }
  render()
  return { speech, sample: (delta) => { render(); frame({}, delta); return render() } }
}

const playback = { take: 1, time: .25 }
const controlled = screen({ clock: () => playback.time })
const paused = controlled.sample(.1)
assert.equal(controlled.sample(.6), paused, 'Paused playback must freeze the face text too')
playback.time = .75
const later = controlled.sample(.01)
playback.time = .25
assert.equal(controlled.sample(.01), paused, 'Backward seek must restore the same typed text')
assert.notEqual(later, paused, 'Forward seek must advance text through the same timeline')
playback.take++
playback.time = 0
assert.equal(controlled.sample(.01), '', 'A new take at zero must restart unchanged speech')
playback.time = .25
assert.equal(controlled.sample(.01), paused, 'Repeating the same speech by take must reproduce its frame')

const reduced = screen({ clock: () => .25, reducedMotion: true })
assert.equal(reduced.sample(.1), reduced.speech.speech, 'Reduced motion retains the full current page without typing')

const returningProps = { clock: () => .75 }
const returning = screen(returningProps)
const finalReactionText = returning.sample(.1)
delete returningProps.clock
assert.notEqual(returning.sample(.1), 'The_', 'Returning to autonomous idle must not replay the old reaction speech')
assert.ok(returning.sample(.1).length >= finalReactionText.length, 'Autonomous speech continues from its last controlled time')

const autonomous = screen()
const before = autonomous.sample(.1)
assert.notEqual(autonomous.sample(.6), before, 'Legacy autonomous speech still follows elapsed time')
autonomous.speech.speechId++
assert.equal(autonomous.sample(.1), before, 'Legacy repeated speech still restarts on speechId')
const held = screen()
held.speech.clip = 'fuckOff'
assert.equal(held.sample(20), held.speech.speech, 'The legacy final reaction keeps its last page')

const firstPage = 'a'.repeat(23) + ' ' + 'b'.repeat(22)
const pages = speechPages(firstPage + ' next page')
assert.deepEqual(pages, [firstPage, 'next page'], 'A 46-character page stays together and later words wrap')
assert.deepEqual(speechPages('  Hello 😀 world  '), ['Hello world'], 'Whitespace and unsupported pictographs retain the legacy font boundary')
const boundary = firstPage.length / 32 + 1.8
assert.equal(speechFrame(pages, boundary, false, false).text, firstPage, 'A page is held through its exact end')
assert.equal(speechFrame(pages, boundary + .125, false, false).text, 'next', 'The next page uses its own elapsed time')
assert.equal(speechFrame(pages, 20, false, false).text, '', 'Ordinary speech clears after the final page')
assert.equal(speechFrame(pages, 20, true, false).text, 'next page', 'The legacy reaction holds only its final page')
assert.deepEqual(speechFrame([], 0, false, false), { text: '', of: 0 }, 'Empty speech has no page or cursor')

// Evaluate each real caller's ScreenText JSX, retaining the async stream binding.
for (const name of ['actions/ScenarioAction', 'market/MarketTrade', 'reactions/LivingGenie']) {
  const file = new URL(`../src/live/${name}.tsx`, import.meta.url)
  const ast = ts.createSourceFile(file.pathname, readFileSync(file, 'utf8'), ts.ScriptTarget.ES2023, true, ts.ScriptKind.TSX)
  let text
  const visit = (node) => {
    if (ts.isJsxSelfClosingElement(node) && node.tagName.getText(ast) === 'ScreenText') text = node
    ts.forEachChild(node, visit)
  }
  visit(ast)
  assert.ok(text, `${name} must render the shared screen`)
  const expression = ts.transpileModule(`module.exports = (${text.getText(ast)})`, {
    compilerOptions: { module: ts.ModuleKind.CommonJS, jsx: ts.JsxEmit.ReactJSX, target: ts.ScriptTarget.ES2023 },
  }).outputText
  const props = (syncSpeech) => {
    const module = { exports: {} }
    runInNewContext(expression, { require, module, exports: module.exports, ScreenText: 'ScreenText',
      syncSpeech, reduced: true, time: { current: 3 }, playback: { current: { time: 3 } },
      reaction: { kind: 'thinking' }, preview: { action: name.startsWith('actions') ? 'bribe' : 'sell' }, actionStageAt, PURCHASE_CONTACT })
    return module.exports.props
  }
  const liveProps = props(false)
  assert.equal(liveProps.clock, undefined, `${name}: asynchronous stream words must not inherit a late action clock`)
  assert.equal(liveProps.reducedMotion, true, `${name}: asynchronous speech retains reduced motion`)
  assert.equal(typeof props(true).clock, 'function', `${name}: pause/seek previews retain the shared clock`)
  const live = screen(liveProps)
  live.sample(3)
  live.speech.speech = 'OK'; live.speech.speechId++
  assert.equal(live.sample(.1), 'OK', `${name}: a new public message must start independently of the ongoing action`)
}

console.log('PASS: ScreenText pause, seek, repeated take, reduced motion, legacy speech and all three async stream caller bindings')
