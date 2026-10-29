// Exercise the real map component and its clock without requiring WebGL, a
// browser or network. The canvas and animation frames are deterministic stubs.
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { runInNewContext } from 'node:vm'
import { setImmediate } from 'node:timers/promises'
import ts from 'typescript'
import { chiefSprite, workerMotion } from '../src/strategies/atlasMotion.ts'

const source = new URL('../src/strategies/Map.tsx', import.meta.url)
const require = createRequire(source)
const jsx = require('react/jsx-runtime')
const input = readFileSync(source, 'utf8').replaceAll('import.meta.env.BASE_URL', "'/'")
const compiled = ts.transpileModule(input, { compilerOptions: { module: ts.ModuleKind.CommonJS, jsx: ts.JsxEmit.ReactJSX, target: ts.ScriptTarget.ES2023 } }).outputText
const layout = { labels: [], chiefs: [[100, 100], [200, 200]], routes: Array.from({ length: 8 }, () => [[100, 100], [101, 102], [103, 104], [105, 106]]) }

function harness(teams, reduced = false, playerControls = true) {
  const hooks = [], pending = []
  let index = 0, frame, tree
  const react = {
    useRef: (value) => hooks[index++] ?? (hooks[index - 1] = { current: value }),
    useState: (value) => {
      const slot = index++
      if (!(slot in hooks)) hooks[slot] = value
      return [hooks[slot], next => { hooks[slot] = typeof next === 'function' ? next(hooks[slot]) : next }]
    },
    useEffect: (effect, deps) => {
      const slot = index++, old = hooks[slot]
      if (!old || deps.some((value, i) => value !== old.deps[i])) pending.push(() => { old?.cleanup?.(); hooks[slot] = { deps, cleanup: effect() } })
    },
  }
  class FakeImage { set src(_) { queueMicrotask(() => this.onload()) } }
  const canvas = { dataset: {} }
  const context = new Proxy({ canvas }, { get: (target, key) => key in target ? target[key] : () => {} })
  canvas.getContext = () => context
  const slider = { value: '0', max: '20', setAttribute() {} }, clock = { textContent: '' }
  const module = { exports: {} }
  runInNewContext(compiled, {
    module, exports: module.exports, Image: FakeImage, AbortController, queueMicrotask,
    fetch: async () => ({ ok: true, json: async () => layout }),
    window: { matchMedia: () => ({ matches: reduced }) },
    requestAnimationFrame: callback => { frame = callback; return 1 }, cancelAnimationFrame() {},
    require: name => name === 'react' ? react : name === 'react/jsx-runtime' ? jsx : name === './timeline' ? { point: () => [100, 100] } : name === './atlasMotion' ? { REPLAY_TAIL_SECONDS: 1.433, chiefSprite, workerMotion } : name.endsWith('.css') ? {} : require(name),
  })
  function elements(node, out = []) {
    if (!node || typeof node !== 'object') return out
    if (Array.isArray(node)) { node.forEach(child => elements(child, out)); return out }
    out.push(node)
    elements(node.props?.children, out)
    return out
  }
  async function render() {
    index = 0
    tree = module.exports.DegenieMap({ teams, online: true, caption: 'Recorded map', playerControls })
    for (const element of elements(tree)) {
      if (element.type === 'canvas' && element.props.ref) element.props.ref.current = canvas
      if (element.type === 'input' && element.props.ref) element.props.ref.current = slider
      if (element.type === 'output' && element.props.ref) element.props.ref.current = clock
    }
    while (pending.length) pending.shift()()
    await setImmediate()
    return tree
  }
  function find(type, label) { return elements(tree).find(element => element.type === type && element.props['aria-label'] === label) }
  function hasLegacyButton(label) { return elements(tree).some(element => element.type === 'button' && element.props.children === label) }
  async function step(ms) { assert.ok(frame, 'Map must request a frame'); frame(ms); await render() }
  return { render, find, step, slider, clock, hasLegacyButton }
}

const game = [{ id: 'team-a', completed: true, stages: [{ task: 'sell', route: 0, start: 0, work: 0, end: .1, ok: true, tool: false }] }]
const map = harness(game)
await map.render()
assert.ok(map.find('button', 'Pause map replay'), 'A playing map needs a clearly labelled pause button')
assert.ok(map.find('button', 'Replay map from the beginning'), 'Replay must be discoverable')
assert.ok(map.find('input', 'Map replay position'), 'A visible, labelled map position is required')
await map.step(1000)
for (let i = 1; i <= 17; i++) await map.step(1000 + i * 100)
assert.ok(map.find('button', 'Play map replay'), 'At the end the map must offer Play rather than Pause')
map.find('input', 'Map replay position').props.onChange({ target: { value: '.4' } })
await map.render()
map.find('button', 'Play map replay').props.onClick()
await map.render(); await map.step(2800)
assert.ok(Number(map.slider.value) >= .4 && Number(map.slider.value) < .6, 'Playing immediately after a backward seek must not restart at zero')
for (let i = 1; i <= 12; i++) await map.step(2800 + i * 100)
assert.ok(map.find('button', 'Play map replay'), 'The map still finishes after seeking back')
map.find('button', 'Play map replay').props.onClick()
await map.render(); await map.step(4000)
assert.equal(Number(map.slider.value), 0, 'Playing from the end starts at the beginning')
await map.step(4100)
map.find('input', 'Map replay position').props.onChange({ target: { value: '.4' } })
await map.render(); await map.step(4200)
assert.equal(Number(map.slider.value), .4, 'Seeking changes the position')
assert.ok(map.find('button', 'Play map replay'), 'Seeking pauses the map')
map.find('button', 'Replay map from the beginning').props.onClick()
await map.render(); await map.step(5000)
assert.equal(Number(map.slider.value), 0, 'Replay resets the position')
assert.ok(map.find('button', 'Pause map replay'), 'Replay starts playback')
const empty = harness([])
await empty.render()
assert.equal(empty.find('button', 'Play map replay').props.disabled, true, 'No events must leave playback disabled')
const manual = harness(game)
await manual.render(); await manual.step(1000)
manual.find('button', 'Pause map replay').props.onClick()
await manual.render(); await manual.step(1100)
assert.equal(Number(manual.slider.value), 0, 'Pause holds the current position')
manual.find('button', 'Play map replay').props.onClick()
await manual.render(); await manual.step(1200)
assert.equal(Number(manual.slider.value), .1, 'Play resumes from the paused position')
const legacy = harness(game, false, false)
await legacy.render()
assert.ok(legacy.hasLegacyButton('Pause replay'), 'Other map consumers retain their existing controls')
const partial = harness([{ ...game[0], completed: false, stages: [{ ...game[0].stages[0], end: null }] }])
await partial.render(); await partial.step(1000)
assert.match(partial.clock.textContent, /—$/, 'An incomplete recording must not be labelled live or given a fabricated duration')
const reduced = harness(game, true)
await reduced.render(); await reduced.step(1000); await reduced.step(1100)
assert.equal(Number(reduced.slider.value), .1, 'Reduced motion retains a usable replay timeline')
reduced.find('input', 'Map replay position').props.onChange({ target: { value: '.4' } })
await reduced.render(); await reduced.step(1200)
assert.equal(Number(reduced.slider.value), .4, 'Reduced motion retains exact manual seeking')
for (let i = 0; i <= 30; i++) {
  const time = i / 10, motion = workerMotion(game[0].stages[0], time, 0, true)
  assert.equal(motion.progress, 0, 'Reduced motion does not move a finished worker along its route')
  assert.ok((motion.clip === 0 && motion.frame === 0) || (motion.clip === 2 && motion.frame === 14), 'Reduced motion uses static atlas poses')
  const chief = chiefSprite(.1, time, true)
  assert.ok((chief.clip === 0 && chief.frame === 0) || (chief.clip === 2 && chief.frame === 14), 'Reduced motion uses a static chief pose')
}
console.log('PASS: map controls, pause on seek, finish, play from end and replay')
