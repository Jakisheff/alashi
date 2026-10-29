// Exercise the actual player controls and seek callback without a browser or GPU.
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { runInNewContext } from 'node:vm'
import { Children, isValidElement } from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import ts from 'typescript'

const source = new URL('../src/devnet/DevnetPage.tsx', import.meta.url)
const require = createRequire(source)
const ast = ts.createSourceFile(source.pathname, readFileSync(source, 'utf8'), ts.ScriptTarget.ES2023, true, ts.ScriptKind.TSX)
let controls, replay, finish, reset, player
function visit(node) {
  if (ts.isFunctionDeclaration(node) && node.name?.text === 'PlayerPlayback') controls = node
  if (ts.isVariableDeclaration(node) && node.name.getText(ast) === 'replay' && node.initializer && ts.isArrowFunction(node.initializer)) replay = node
  if (ts.isVariableDeclaration(node) && node.name.getText(ast) === 'finish' && node.initializer && ts.isCallExpression(node.initializer)) finish = node.initializer.arguments[0]
  if (ts.isVariableDeclaration(node) && node.name.getText(ast) === 'reset' && node.initializer && ts.isCallExpression(node.initializer)) reset = node.initializer.arguments[0]
  if (ts.isJsxSelfClosingElement(node) && node.tagName.getText(ast) === 'PlayerPlayback') player = node
  ts.forEachChild(node, visit)
}
visit(ast)
assert.ok(controls, 'Player replay needs a visible, accessible control panel')
assert.ok(replay, 'Player replay needs its existing event seek callback')
assert.ok(finish && ts.isArrowFunction(finish), 'Player replay needs its event completion callback')
assert.ok(reset && ts.isArrowFunction(reset), 'Player replay needs its existing reset callback')
assert.ok(player, 'Player controls must be connected to the actual game viewer')
const compile = text => ts.transpileModule(text, { compilerOptions: { module: ts.ModuleKind.CommonJS, jsx: ts.JsxEmit.ReactJSX, target: ts.ScriptTarget.ES2023 } }).outputText
function component(props) {
  const module = { exports: {} }
  runInNewContext(compile(`${controls.getText(ast)}\nmodule.exports = PlayerPlayback`), { require, module, exports: module.exports })
  return module.exports(props)
}
function descendants(element) {
  if (!isValidElement(element)) return []
  return [element, ...Children.toArray(element.props.children).flatMap(descendants)]
}
const calls = []
const base = { mode: 'replay', playing: false, current: true, position: 1, total: 3, available: true,
  onToggle: () => calls.push('toggle'), onReplay: () => calls.push('replay'), onLatest: () => calls.push('latest'), onSeek: n => calls.push(n) }
const panel = component(base)
const html = renderToStaticMarkup(panel)
assert.match(html, /Player replay/, 'The player controls identify their separate playback')
assert.match(html, /Action 2 \/ 3/, 'Progress is expressed as confirmed actions, not invented seconds')
const elements = descendants(panel)
const named = (type, name) => elements.find(e => e.type === type && e.props['aria-label'] === name)
const seek = named('input', 'Replay action')
assert.equal(seek?.props.type, 'range', 'An accessible visible range allows event seeking')
assert.equal(seek.props.value, 1)
seek.props.onChange({ target: { value: '0' } })
named('button', 'Resume')?.props.onClick()
named('button', 'Replay confirmed actions')?.props.onClick()
named('button', 'Return to latest state')?.props.onClick()
assert.deepEqual(calls, [0, 'toggle', 'replay', 'latest'], 'Controls route to the replay actions')
const unavailable = descendants(component({ ...base, total: 0, available: false }))
assert.ok(unavailable.filter(e => e.type === 'button' || e.type === 'input').every(e => e.props.disabled), 'No controls operate before confirmed events load')
const playing = descendants(component({ ...base, playing: true }))
assert.ok(playing.some(e => e.type === 'button' && e.props['aria-label'] === 'Pause'), 'Playing shows an accessible pause')
const liveHtml = renderToStaticMarkup(component({ ...base, mode: 'live', playing: false, current: false }))
assert.match(liveHtml, /3 actions available/, 'Live mode offers the recorded actions without pretending the live game is paused')

function wiredActions(mode, position) {
  const calls = [], module = { exports: {} }, replayEvents = ['a', 'b', 'c'], activeEvents = ['a', 'b', 'c']
  runInNewContext(compile(`module.exports = (${player.getText(ast)})`), {
    require, module, exports: module.exports, PlayerPlayback: () => null, mode, position, replayEvents, activeEvents,
    playing: false, playingRef: { current: false }, deferredFinish: { current: null }, take: 7, finish: () => calls.push(['finish']), current: {}, snapshot: {}, replay: (...args) => calls.push(['replay', ...args]),
    setPlaying: value => calls.push(['playing', value]), reset: () => calls.push(['latest']),
  })
  return { props: module.exports.props, calls }
}
const live = wiredActions('live', -1)
live.props.onToggle(); live.props.onSeek(1)
assert.equal(JSON.stringify(live.calls), JSON.stringify([['replay'], ['replay', 1, true]]), 'Play or seek from live opens a replay, not a pause of the game')
const activeReplay = wiredActions('replay', 1)
activeReplay.props.onToggle(); activeReplay.props.onLatest()
assert.equal(JSON.stringify(activeReplay.calls), JSON.stringify([['playing', true], ['latest']]), 'Paused replay resumes and Latest returns to live')
const endedReplay = wiredActions('replay', 3)
endedReplay.props.onToggle()
assert.equal(JSON.stringify(endedReplay.calls), JSON.stringify([['replay']]), 'Play after the last event restarts from the beginning')

function pauseHarness() {
  const module = { exports: {} }, replayEvents = ['a', 'b', 'c'], activeEvents = replayEvents
  const completedTake = { current: -1 }, playingRef = { current: true }, deferredFinish = { current: null }
  let statePlaying = true, position = 1
  const setPlaying = value => { statePlaying = typeof value === 'function' ? value(statePlaying) : value }
  runInNewContext(compile(`const finish = ${finish.getText(ast)}\nconst controls = (${player.getText(ast)})\nmodule.exports = { finish, controls }`), {
    require, module, exports: module.exports, PlayerPlayback: () => null, mode: 'replay', playing: true, position,
    replayEvents, activeEvents, current: {}, snapshot: {}, completedTake, playingRef, deferredFinish, take: 7,
    replay: () => { throw new Error('A paused take must resume without restarting') }, reset: () => {}, setPlaying,
    setPosition: value => { position = typeof value === 'function' ? value(position) : value },
  })
  return { ...module.exports, state: () => ({ playing: statePlaying, position }) }
}
const pause = pauseHarness()
pause.controls.props.onToggle()
assert.equal(pause.state().playing, false, 'Pause stops the active replay')
pause.finish()
assert.equal(pause.state().position, 1, 'A completion captured before Pause cannot advance while paused')
pause.controls.props.onToggle()
assert.equal(pause.state().playing, true, 'Resume continues the same take')
assert.equal(pause.state().position, 2, 'Resume consumes the one-shot completion deferred during Pause')
pause.finish()
assert.equal(pause.state().position, 2, 'The one-shot completion advances at most once')

function seekState(mode, playing, index, seek) {
  const calls = {}, snapshot = { marker: 'snapshot' }, replayEvents = ['a', 'b', 'c'], observed = ['x', 'y']
  const queue = { current: [] }, liveEvent = { current: null }
  const capture = name => value => { calls[name] = value }
  const module = { exports: {} }
  runInNewContext(compile(`const ${replay.getText(ast)}\nmodule.exports = replay`), {
    module, exports: module.exports, mode, playing, snapshot, replayEvents, observed, queue, liveEvent,
    completedTake: { current: -1 }, playingRef: { current: playing }, deferredFinish: { current: null }, take: 7,
    setReplaySnapshot: capture('snapshot'), setReplayEvents: capture('events'), setCurrent: capture('current'),
    setMode: capture('mode'), setPosition: capture('position'), setPlaying: capture('playing'), setReplayRevision: capture('revision'),
  })
  module.exports(index, seek)
  return calls
}
const pausedSeek = seekState('replay', false, 2, true)
assert.equal(pausedSeek.playing, false, 'Seeking while paused stays paused')
assert.equal(pausedSeek.events.length, 3, 'Seeking retains the frozen confirmed history')
assert.equal(pausedSeek.position, 2)
assert.equal(seekState('replay', true, 0, true).playing, true, 'Seeking during playback continues playback')
assert.equal(seekState('replay', false, 0, false).playing, true, 'Replay from the beginning starts playing')
assert.equal(seekState('live', false, 1, true).events.length, 2, 'Seeking from live freezes the observed history')

function completionHarness(playing, state = { position: 0, completedTake: { current: -1 } }, take = 7) {
  const { completedTake } = state, snapshot = {}, replayEvents = ['a', 'b', 'c'], observed = ['a', 'b', 'c']
  const queue = { current: [] }, liveEvent = { current: null }, deferredFinish = { current: null }, module = { exports: {} }
  const ignore = () => {}
  runInNewContext(compile(`const finish = ${finish.getText(ast)}\nconst ${replay.getText(ast)}\nconst reset = ${reset.getText(ast)}\nmodule.exports = { finish, replay, reset }`), {
    module, exports: module.exports, mode: 'replay', playing, snapshot, replayEvents, observed, queue, liveEvent,
    completedTake, playingRef: { current: playing }, deferredFinish, take, takeRef: { current: take }, useScene: { getState: () => ({ say: ignore }) }, setPosition: value => { state.position = typeof value === 'function' ? value(state.position) : value },
    setReplaySnapshot: ignore, setReplayEvents: ignore, setCurrent: ignore, setMode: ignore, setPlaying: ignore, setReplayRevision: ignore,
  })
  return { ...module.exports, position: () => state.position, deferredFinish }
}
const active = completionHarness(true)
active.finish(); active.finish()
assert.equal(active.position(), 1, 'Repeated completion cannot skip an event')
const paused = completionHarness(false)
paused.finish()
assert.equal(paused.position(), 0, 'A paused event cannot advance')
assert.equal(paused.deferredFinish.current, 7, 'A one-shot completion while paused is held for that take')
paused.replay(1, true)
assert.equal(paused.deferredFinish.current, null, 'Seeking clears a deferred completion')
paused.finish()
assert.equal(paused.deferredFinish.current, null, 'An old completion cannot return after seeking')
const pendingReset = completionHarness(false)
pendingReset.finish(); pendingReset.reset(); pendingReset.finish()
assert.equal(pendingReset.deferredFinish.current, null, 'Latest or player change clears and invalidates a deferred completion')
const afterLatest = completionHarness(true)
afterLatest.reset(); afterLatest.finish()
assert.equal(afterLatest.position(), -1, 'Returning to latest invalidates a late completion from the previous replay')
const seeking = completionHarness(true)
seeking.replay(2, true)
seeking.finish()
assert.equal(seeking.position(), 2, 'A completion from the prior event cannot advance a newly selected event')
const shared = { position: 0, completedTake: { current: -1 } }
const previousTake = completionHarness(true, shared, 7)
previousTake.replay(1, true)
const selectedTake = completionHarness(true, shared, 8)
previousTake.finish()
assert.equal(shared.position, 1, 'Old completion cannot skip the selected event')
selectedTake.finish()
assert.equal(shared.position, 2, 'The selected event advances exactly once on its own completion')
previousTake.finish()
selectedTake.finish()
assert.equal(shared.position, 2, 'Even late and repeated completions cannot skip the next event')
console.log('PASS: accessible controls, frozen history, pause/seek, replay restart and stale or repeated completion')
