// Render the real /stream hero JSX without a GPU or network. CSS keys on this
// boundary; the renderer itself is a canvas stub, not a visual acceptance test.
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { createRequire } from 'node:module'
import { runInNewContext } from 'node:vm'
import { Children, createElement } from 'react'
import { renderToStaticMarkup } from 'react-dom/server'
import ts from 'typescript'

const source = new URL('../src/live/StreamPage.tsx', import.meta.url)
const require = createRequire(source)
const ast = ts.createSourceFile(source.pathname, readFileSync(source, 'utf8'), ts.ScriptTarget.ES2023, true, ts.ScriptKind.TSX)
let hero
function visit(node) {
  if (ts.isJsxElement(node) && node.openingElement.attributes.properties.some((attribute) =>
    ts.isJsxAttribute(attribute) && attribute.name.getText(ast) === 'className' && attribute.initializer?.text === 'live-hero')) hero = node
  ts.forEachChild(node, visit)
}
visit(ast)
assert.ok(hero, 'The public stream must contain its hero stage')
const compile = (text) => ts.transpileModule(text, {
  compilerOptions: { module: ts.ModuleKind.CommonJS, jsx: ts.JsxEmit.ReactJSX, target: ts.ScriptTarget.ES2023 },
}).outputText
const bubbleModule = { exports: {} }
runInNewContext(compile(readFileSync(new URL('../src/live/ThoughtBubble.tsx', import.meta.url), 'utf8')), {
  require, exports: bubbleModule.exports, module: bubbleModule,
})
const { ThoughtBubble } = bubbleModule.exports
const compiled = compile(`module.exports = (${hero.getText(ast)})`)
const through = ({ children }) => children
const ignore = () => {}
function frame({ kind = 'thinking', time = 2.65, trade = null, visualTime = 0, tradeTime = 0 } = {}) {
  const module = { exports: {} }
  runInNewContext(compiled, {
    require, exports: module.exports, module,
    heroElement: null, trade, tradeTime, visualTime, reportVisualTime: ignore, reaction: kind ? { kind } : null, reactionTime: time,
    sharedThought: 'The public choice needs more thought.', ThoughtBubble,
    StreamHeroBoundary: through, Suspense: through, StudioStage: through,
    Scene: () => createElement('canvas', { 'aria-label': '3D scene' }),
    MarketTrade: ignore, ScenarioAction: ignore, LivingGenie: ignore, PurchaseReceipt: ignore,
    reportTradeTime: ignore, finish: ignore, ignoreTime: ignore, setReactionTime: ignore, finishReaction: ignore,
  })
  return module.exports
}

const thinking = frame()
assert.equal(thinking.props['data-reaction'], 'thinking', 'A real thinking cue must enable the existing face-clearance CSS')
const children = Children.toArray(thinking.props.children)
const scene = children.find((child) => child.props.className === 'live-scene')
assert.ok(scene, 'The real scene must occupy the same movable wrapper as the preview')
assert.ok(children.some((child) => child.type === ThoughtBubble), 'The thought stays outside the shifted scene')
const html = renderToStaticMarkup(thinking)
assert.match(html, /data-reaction="thinking"/)
assert.match(html, /class="live-scene"/)
assert.match(html, /aria-label="Shared thought"/)
assert.ok(html.includes('The public choice needs more thought.'), 'The public thought text is preserved')
for (const time of [.59, 4.5]) assert.ok(!renderToStaticMarkup(frame({ time })).includes('aria-label="Shared thought"'), 'The existing thought display window is preserved')
assert.ok(!renderToStaticMarkup(frame({ trade: { action: 'buy' } })).includes('aria-label="Shared thought"'), 'A trade keeps priority over a thought')
const buy = frame({ trade: { action: 'buy', purchase: { source: 'arena' } }, tradeTime: 1, visualTime: 4.25 })
const receipt = Children.toArray(buy.props.children).find(child => child.type === ignore)
assert.equal(receipt?.props.time, 4.25, 'The public receipt follows the displayed pose, not actual playback time')
assert.equal(frame({ kind: null }).props['data-reaction'], 'idle', 'Returning to idle removes the thinking shift')
console.log('PASS: actual /stream hero markup activates thinking clearance, keeps the thought outside the scene and preserves its visibility window')
