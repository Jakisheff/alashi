// Exercise the real map component and its clock without requiring WebGL, a
// browser or network. The canvas and animation frames are deterministic stubs.
import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { readFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { runInNewContext } from 'node:vm'
import { setImmediate } from 'node:timers/promises'
import ts from 'typescript'
import { chiefSprite, workerMotion } from '../src/strategies/atlasMotion.ts'
import { point } from '../src/strategies/timeline.ts'

const source = new URL('../src/strategies/Map.tsx', import.meta.url)
const require = createRequire(source)
const jsx = require('react/jsx-runtime')
const input = readFileSync(source, 'utf8').replaceAll('import.meta.env.BASE_URL', "'/'")
const compiled = ts.transpileModule(input, { compilerOptions: { module: ts.ModuleKind.CommonJS, jsx: ts.JsxEmit.ReactJSX, target: ts.ScriptTarget.ES2023 } }).outputText
const layout = { labels: [['MARKET', 300, 400], ['ANALYTICS', 500, 600], ['VERIFY A', 700, 800], ['PARLIAMENT', 900, 100]], chiefs: [[100, 100], [200, 200]], routes: Array.from({ length: 8 }, (_, i) => [[100 + i, 100], [101 + i, 102], [103 + i, 104], [105 + i, 106]]) }

function harness(teams, reduced = false, playerControls = true, contextAvailable = true, failFirstFetch = false, trading = false, online = true, lifecycle = null, getOnTime = () => undefined, failFirstImage = false) {
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
  let fetchCount = 0, releaseRetry
  const retryGate = new Promise(resolve => { releaseRetry = resolve })
  let imageFailed = false
  class FakeImage {
    set src(url) {
      this.url = url
      queueMicrotask(() => {
        if (failFirstImage && !imageFailed) { imageFailed = true; this.onerror() }
        else this.onload()
      })
    }
  }
  const canvas = { dataset: {} }
  const draws = []
  const context = new Proxy({ canvas }, {
    get: (target, key) => key in target ? target[key] : (...args) => draws.push([key, ...args.map(value => value instanceof FakeImage ? value.url : value)]),
    set: (target, key, value) => { draws.push(['set', key, value]); target[key] = value; return true },
  })
  canvas.getContext = () => (typeof contextAvailable === 'function' ? contextAvailable() : contextAvailable) ? context : null
  const slider = { value: '0', max: '20', setAttribute() {} }, clock = { textContent: '' }
  const module = { exports: {} }
  runInNewContext(compiled, {
    module, exports: module.exports, Image: FakeImage, AbortController, queueMicrotask,
    fetch: async (url, options) => {
      if (lifecycle) return lifecycle.fetch(url, options)
      fetchCount++
      if (failFirstFetch && fetchCount === 1) return { ok: false }
      if (failFirstFetch) await retryGate
      return { ok: true, json: async () => layout }
    },
    window: { matchMedia: () => ({ matches: reduced }) },
    requestAnimationFrame: callback => { frame = callback; return lifecycle ? lifecycle.schedule(callback) : 1 },
    cancelAnimationFrame: id => { lifecycle?.cancel(id) },
    require: name => name === 'react' ? react : name === 'react/jsx-runtime' ? jsx : name === './timeline' ? { point } : name === './atlasMotion' ? { REPLAY_TAIL_SECONDS: 1.433, chiefSprite, workerMotion } : name.endsWith('.css') ? {} : require(name),
  })
  function elements(node, out = []) {
    if (!node || typeof node !== 'object') return out
    if (Array.isArray(node)) { node.forEach(child => elements(child, out)); return out }
    out.push(node)
    if (typeof node.type === 'function') elements(node.type(node.props), out)
    else elements(node.props?.children, out)
    return out
  }
  async function render() {
    index = 0
    tree = module.exports.DegenieMap({ teams, online: typeof online === 'function' ? online() : online, caption: 'Recorded map', playerControls, trading, onTime: getOnTime() })
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
  function alert() { return elements(tree).find(element => element.props?.role === 'alert') }
  function legacyButton(label) { return elements(tree).find(element => element.type === 'button' && element.props.children === label) }
  async function step(ms) { assert.ok(frame, 'Map must request a frame'); frame(ms); await render() }
  function unmount() { for (const hook of hooks) hook?.cleanup?.() }
  return { render, find, step, slider, clock, legacyButton, alert, releaseRetry, draws, unmount, canvas }
}

function lifecycleProbe() {
  const requests = [], frames = [], cancelled = []
  return {
    requests, frames, cancelled,
    fetch: (_url, { signal }) => new Promise(resolve => requests.push({ signal, resolve })),
    schedule: callback => { frames.push(callback); return frames.length },
    cancel: id => cancelled.push(id),
  }
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
manual.find('input', 'Map replay position').props.onChange({ target: { value: '0' } })
await manual.render(); await manual.step(1300)
assert.equal(Number(manual.slider.value), 0, 'Seeking to zero preserves zero as a real position')
manual.find('button', 'Play map replay').props.onClick()
await manual.render(); await manual.step(1400)
assert.equal(Number(manual.slider.value), .1, 'Play advances from a zero seek')
let connected = true
const reconnecting = harness(game, false, true, true, false, false, () => connected)
await reconnecting.render(); await reconnecting.step(1000); await reconnecting.step(1100)
connected = false; await reconnecting.render(); await reconnecting.step(1200)
assert.equal(Number(reconnecting.slider.value), .1, 'Disconnection freezes the map at its previous position')
assert.equal(reconnecting.find('button', 'Play map replay').props.disabled, true, 'Playback is unavailable offline')
assert.equal(reconnecting.find('input', 'Map replay position').props.disabled, false, 'Offline alone does not disable seeking within loaded history')
connected = true; await reconnecting.render(); await reconnecting.step(1300)
assert.equal(Number(reconnecting.slider.value), .2, 'The map continues after reconnection')
const legacy = harness(game, false, false)
await legacy.render()
assert.ok(legacy.legacyButton('Pause replay'), 'Other map consumers retain their existing controls')
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
const unavailableCanvas = harness(game, false, true, false)
await unavailableCanvas.render(); await unavailableCanvas.render()
assert.ok(unavailableCanvas.alert(), 'Canvas initialization failure must be reported to the viewer')
assert.equal(unavailableCanvas.find('button', 'Play map replay').props.disabled, true, 'An unavailable canvas cannot play')
assert.equal(unavailableCanvas.find('button', 'Replay map from the beginning').props.disabled, true, 'An unavailable canvas cannot replay')
assert.equal(unavailableCanvas.find('input', 'Map replay position').props.disabled, true, 'An unavailable canvas cannot seek')
const unavailableLegacy = harness(game, false, false, false)
await unavailableLegacy.render(); await unavailableLegacy.render()
assert.ok(unavailableLegacy.alert(), 'Other map consumers also report unavailable canvas')
assert.equal(unavailableLegacy.legacyButton('Pause replay').props.disabled, true, 'Other map consumers cannot play without canvas')
assert.equal(unavailableLegacy.legacyButton('Replay events').props.disabled, true, 'Other map consumers cannot replay without canvas')
assert.equal(unavailableLegacy.find('input', 'Replay position in seconds').props.disabled, true, 'Other map consumers cannot seek without canvas')
let contextRestored = false
const newTeams = [{ ...game[0] }]
const restored = harness(newTeams, false, true, () => contextRestored)
await restored.render(); await restored.render()
contextRestored = true; newTeams[0] = { ...newTeams[0], id: 'new-team' }
await restored.render(); await restored.render()
assert.equal(restored.alert(), undefined, 'A new map with an available canvas clears the error')
assert.equal(restored.find('button', 'Pause map replay').props.disabled, false, 'Playback becomes available when the canvas is restored')
for (const playerControls of [true, false]) {
  const retry = harness(game, false, playerControls, true, true)
  await retry.render(); await retry.render()
  assert.ok(retry.alert(), 'Failed map asset request is reported')
  const toggle = () => playerControls ? retry.find('button', 'Play map replay') ?? retry.find('button', 'Pause map replay') : retry.legacyButton('Play replay') ?? retry.legacyButton('Pause replay')
  const position = () => retry.find('input', playerControls ? 'Map replay position' : 'Replay position in seconds')
  const replayButton = playerControls ? retry.find('button', 'Replay map from the beginning') : retry.legacyButton('Replay events')
  assert.equal(toggle().props.disabled, true, 'A failed map cannot be played or paused')
  assert.equal(position().props.disabled, true, 'A failed map cannot be sought')
  assert.equal(replayButton.props.disabled, false, 'Replay remains available to retry the failed assets')
  replayButton.props.onClick()
  await retry.render(); await retry.render()
  assert.ok(retry.alert(), 'The error remains visible while assets are still loading')
  assert.equal(toggle().props.disabled, true, 'A retry cannot play until assets finish loading')
  assert.equal(position().props.disabled, true, 'A retry cannot seek until assets finish loading')
  retry.releaseRetry()
  await retry.render(); await retry.render()
  assert.equal(retry.alert(), undefined, 'A successful map reload clears the asset error')
  assert.equal(toggle().props.disabled, false, 'Successful asset retry restores playback')
  assert.equal(position().props.disabled, false, 'Successful asset retry restores seeking')
  await retry.step(1000)
}
for (const playerControls of [true, false]) {
  const offlineFailure = harness(game, false, playerControls, true, true, false, false)
  await offlineFailure.render(); await offlineFailure.render()
  assert.ok(offlineFailure.alert(), 'Failed assets remain reported while offline')
  const replayButton = playerControls ? offlineFailure.find('button', 'Replay map from the beginning') : offlineFailure.legacyButton('Replay events')
  assert.equal(replayButton.props.disabled, playerControls, 'Asset failure does not change the existing offline replay policy')
}
for (const playerControls of [true, false]) {
  const retryImage = harness(game, false, playerControls, true, false, false, true, null, () => undefined, true)
  await retryImage.render(); await retryImage.render()
  assert.equal(retryImage.alert()?.props.children, 'Map assets could not load.', 'Failed image load is reported')
  const toggle = playerControls ? retryImage.find('button', 'Play map replay') : retryImage.legacyButton('Pause replay')
  const position = retryImage.find('input', playerControls ? 'Map replay position' : 'Replay position in seconds')
  const replayButton = playerControls ? retryImage.find('button', 'Replay map from the beginning') : retryImage.legacyButton('Replay events')
  assert.equal(toggle.props.disabled, true, 'An image failure disables playback')
  assert.equal(position.props.disabled, true, 'An image failure disables seeking')
  assert.equal(replayButton.props.disabled, false, 'Replay can retry after an image failure')
  replayButton.props.onClick()
  await retryImage.render(); await retryImage.render()
  assert.equal(retryImage.alert(), undefined, 'A successful image retry clears the error')
  await retryImage.step(1000)
}
const successResponse = { ok: true, json: async () => layout }
const failedResponse = { ok: false }
const oldSuccessProbe = lifecycleProbe(), oldSuccessTeams = [{ ...game[0] }]
const oldSuccess = harness(oldSuccessTeams, false, true, true, false, false, true, oldSuccessProbe)
await oldSuccess.render()
oldSuccessTeams[0] = { ...oldSuccessTeams[0], id: 'new-team' }
await oldSuccess.render()
assert.equal(oldSuccessProbe.requests[0].signal.aborted, true, 'Changing teams aborts the previous request')
oldSuccessProbe.requests[1].resolve(failedResponse)
await oldSuccess.render(); await oldSuccess.render()
assert.ok(oldSuccess.alert(), 'The current load failure is reported')
oldSuccessProbe.requests[0].resolve(successResponse)
await oldSuccess.render(); await oldSuccess.render()
assert.ok(oldSuccess.alert(), 'A stale success cannot clear the current error')
assert.equal(oldSuccessProbe.frames.length, 0, 'A stale success cannot start animation')
oldSuccess.unmount()

const oldFailureProbe = lifecycleProbe(), oldFailureTeams = [{ ...game[0] }]
const oldFailure = harness(oldFailureTeams, false, true, true, false, false, true, oldFailureProbe)
await oldFailure.render()
oldFailureTeams[0] = { ...oldFailureTeams[0], id: 'new-team' }
await oldFailure.render()
oldFailureProbe.requests[1].resolve(successResponse)
await oldFailure.render(); await oldFailure.render()
assert.equal(oldFailure.alert(), undefined, 'The current successful load has no error')
assert.equal(oldFailureProbe.frames.length, 1, 'The current successful load schedules one frame')
oldFailureProbe.requests[0].resolve(failedResponse)
await oldFailure.render(); await oldFailure.render()
assert.equal(oldFailure.alert(), undefined, 'A stale failure cannot replace the current success')
assert.equal(oldFailureProbe.frames.length, 1, 'A stale failure cannot schedule frames')
oldFailure.unmount()
assert.equal(oldFailureProbe.requests[1].signal.aborted, true, 'Unmount aborts the current request')
assert.ok(oldFailureProbe.cancelled.includes(1), 'Unmount cancels the scheduled frame')
oldFailureProbe.frames[0](1000)
assert.equal(oldFailureProbe.frames.length, 1, 'A frame delivered after unmount cannot reschedule itself')
const frameProbe = lifecycleProbe(), reportedByFirst = [], reportedBySecond = []
let onTime = t => reportedByFirst.push(t)
const frameLifecycle = harness(game, false, true, true, false, false, true, frameProbe, () => onTime)
await frameLifecycle.render()
frameProbe.requests[0].resolve(successResponse)
await frameLifecycle.render()
frameProbe.frames[0](1000)
await frameLifecycle.render()
onTime = t => reportedBySecond.push(t)
await frameLifecycle.render()
frameProbe.frames[1](1100)
await frameLifecycle.render()
assert.deepEqual(reportedByFirst, [0], 'The first frame uses the original onTime callback')
assert.deepEqual(reportedBySecond, [.1], 'The next frame uses the updated onTime callback')
assert.equal(frameProbe.frames.length, 3, 'Each completed frame schedules its successor')
frameLifecycle.unmount()
assert.ok(frameProbe.cancelled.includes(3), 'Unmount cancels the latest scheduled frame, not an earlier one')
frameProbe.frames[2](1200)
assert.equal(frameProbe.frames.length, 3, 'A late latest-frame callback cannot restart an unmounted loop')
const representative = [
  { id: 'a', completed: true, stages: [{ task: 'sell confirmed', route: 0, start: 0, work: .8, end: 2.4, ok: true, tool: true }, { task: 'analysis', route: 2, start: .2, work: 1, end: 2.8, ok: false, tool: false }] },
  { id: 'b', completed: false, stages: [{ task: 'produce confirmed', route: 0, start: .1, work: .9, end: 2.5, ok: true, tool: true }, { task: 'tool', route: 2, start: 0, work: .8, end: null, ok: false, tool: true }] },
]
const goldenFrames = []
for (const [trading, reduced, time, tasks] of [
  [false, false, 0, 'sell confirmed,tool'],
  [true, false, 1.2, 'sell confirmed,analysis,produce confirmed,tool'],
  [true, false, 2.1, 'sell confirmed,analysis,produce confirmed,tool'],
  [false, true, 3.1, 'tool'],
]) {
  const scene = harness(representative, reduced, true, true, false, trading)
  await scene.render(); await scene.step(1000)
  scene.find('input', 'Map replay position').props.onChange({ target: { value: String(time) } })
  await scene.render()
  scene.draws.length = 0
  await scene.step(1100)
  assert.equal(scene.canvas.dataset.motionTime, time.toFixed(3), 'The dataset reports the displayed time')
  assert.equal(scene.canvas.dataset.activeTasks, tasks, 'The dataset reports only tasks active at the displayed time')
  assert.ok(scene.draws.length > 100, 'The golden frame must contain terrain, labels and both teams')
  goldenFrames.push(createHash('sha256').update(JSON.stringify(scene.draws)).digest('hex'))
}
assert.deepEqual(goldenFrames, [
  'a10801a8a3c98845b15600702db56d5ecbc868b6fa72b60395668595ef019c37',
  'd01744ef10948bb2eff3dd21e569c99066ac575938c7d0dcc253d401e2a4deb6',
  '79a6c4e111bac4fd8104d8594c0d778b6443f6eddc6d5d56d1cbdcb80b26ca97',
  '778696cd4c2b2d9c1a92eaaa0b60326e4f6154abc8635fdca7cf2927af120769',
], 'Canvas draw calls must preserve the accepted map poses, routes and labels')
const taskBoundaries = harness(representative)
await taskBoundaries.render(); await taskBoundaries.step(1000)
for (const [time, tasks] of [[2.4, 'analysis,produce confirmed,tool'], [2.5, 'analysis,tool'], [2.8, 'tool']]) {
  taskBoundaries.find('input', 'Map replay position').props.onChange({ target: { value: String(time) } })
  await taskBoundaries.render(); await taskBoundaries.step(1100)
  assert.equal(taskBoundaries.canvas.dataset.activeTasks, tasks, 'A completed task is no longer active at its end')
}
console.log('PASS: map controls, pause on seek, finish, replay and unavailable canvas')
