// Real-renderer, local-fixture review. It does not contact an arena or sign anything.
import assert from 'node:assert/strict'
import { mkdir, writeFile } from 'node:fs/promises'
import { fileURLToPath } from 'node:url'
import path from 'node:path'
import { build, preview } from 'vite'
import react from '@vitejs/plugin-react'

const { chromium } = await import(process.env.ALASHI_PLAYWRIGHT_MODULE ?? 'playwright')
const frontend = fileURLToPath(new URL('../../', import.meta.url))
const output = path.resolve(frontend, '../docs/ops/state-machines-20261107/fixes-6-7/visual')
const adapters = fileURLToPath(new URL('./visual-adapters.tsx', import.meta.url))
const config = {
  root: frontend, configFile: false, envDir: false,
  build: { outDir: path.join(frontend, 'node_modules/.visual-review'), emptyOutDir: true, rollupOptions: { input: fileURLToPath(new URL('./visual.html', import.meta.url)) } },
  preview: { host: '127.0.0.1', port: 0, strictPort: true },
  define: { 'import.meta.env.VITE_LIVE_API_ENABLED': '"true"', 'import.meta.env.VITE_LIVE_API_PATH': '""' },
  plugins: [{
    name: 'visual-fixture-boundaries', enforce: 'pre',
    resolveId(source, importer) {
      if (importer?.split('?')[0] === path.join(frontend, 'src/live/StreamPage.tsx') && ['@tanstack/react-router', '../genie/Scene', './api/OwnerPanel'].includes(source)) return adapters
    },
  }, react()],
}
let server
const record = 'a'.repeat(64)
const publicEvent = (seq, action) => ({ seq, event_id: `visual-action-${seq}`, visibility: 'public', kind: 'game_action', room_id: 'game:2', game_id: 2, round: 1, phase: 'market', server_created_at: '2026-11-07T00:00:00Z', branch_id: 'MAIN', finality: 'final', action_ref: { seq, actor: 0, action } })
const arenaEvent = (seq, action) => ({ seq, event_id: `visual:${seq}`, round: 1, phase: 'action', actor: 0, action, by: 'synthetic local visual fixture', ok: true, ts: seq })
const observations = { runtime: {}, pages: [] }
let browser
async function open(mode, width) {
  const viewport = { width, height: width === 390 ? 844 : 1000 }
  const page = await browser.newPage({ viewport, deviceScaleFactor: 1, recordVideo: { dir: path.join(output, '.capture'), size: viewport } })
  page.setDefaultTimeout(30_000)
  const evidence = { mode, width, errors: [], consoleErrors: [], blockedExternal: [], frames: [], trace: [], gpu: null }
  observations.pages.push(evidence)
  page.on('pageerror', e => evidence.errors.push(e.message))
  page.on('console', m => { if (m.type() === 'error') evidence.consoleErrors.push(m.text()) })
  await page.route('**/*', route => {
    const url = new URL(route.request().url())
    if (url.origin === server.resolvedUrls.local[0].replace(/\/$/, '')) return route.continue()
    evidence.blockedExternal.push(url.href)
    return route.abort('blockedbyclient')
  })
  await page.goto(`${server.resolvedUrls.local[0]}scripts/state-repros/visual.html?visual=${mode}&agent=${record}`, { waitUntil: 'domcontentloaded' })
  await page.waitForFunction(() => window.__visualState?.ready)
  await page.locator('canvas').waitFor()
  await page.waitForTimeout(2500)
  evidence.gpu = await page.evaluate(() => {
    const canvas = document.querySelector('canvas')
    const gl = canvas?.getContext('webgl2') ?? canvas?.getContext('webgl')
    if (!gl) return null
    const info = gl.getExtension('WEBGL_debug_renderer_info')
    return { version: gl.getParameter(gl.VERSION), renderer: info ? gl.getParameter(info.UNMASKED_RENDERER_WEBGL) : gl.getParameter(gl.RENDERER) }
  })
  assert(evidence.gpu, 'real WebGL context is required')
  return { page, evidence }
}
async function frame(page, evidence, label) {
  const file = `${evidence.mode}-${evidence.width}-${label}.png`
  await page.screenshot({ path: path.join(output, file), fullPage: true })
  evidence.frames.push(file)
}
async function close(page, evidence) {
  evidence.trace = await page.evaluate(() => window.__visualState.sceneLog)
  const video = page.video()
  await page.close()
  evidence.video = `${evidence.mode}-${evidence.width}.webm`
  await video.saveAs(path.join(output, evidence.video))
  await video.delete()
}
async function stream(width) {
  const { page, evidence } = await open('stream', width)
  try {
    await page.getByText('Feed · connected', { exact: true }).waitFor()
    await frame(page, evidence, 'idle')
    await page.evaluate(e => window.__visualState.publicEvents.push(e), publicEvent(1, 'buy'))
    await page.waitForSelector('.live-hero[data-action="buy"]')
    await page.waitForSelector('.live-purchase-receipt')
    await frame(page, evidence, 'buy-result')
    await page.waitForSelector('.live-hero[data-action="idle"]')
    await page.evaluate(e => window.__visualState.publicEvents.push(e), publicEvent(2, 'sell'))
    await page.waitForSelector('.live-hero[data-action="sell"]')
    await page.waitForTimeout(800)
    await page.evaluate(() => window.dispatchEvent(new Event('visual-renderer-failure')))
    await page.getByText('3D is unavailable. Your stream continues.', { exact: true }).waitFor()
    await page.waitForSelector('.live-hero[data-action="idle"]')
    await page.evaluate(e => window.__visualState.publicEvents.push(e), publicEvent(3, 'bribe'))
    await page.waitForSelector('[data-event-id="visual-action-3"]')
    assert.equal(await page.locator('.live-purchase-receipt').count(), 0)
    assert.equal(await page.locator('.live-hero').getAttribute('data-action'), 'idle')
    await frame(page, evidence, 'fallback-feed')
    evidence.receipts = await page.locator('.live-event').evaluateAll(els => els.map(el => el.getAttribute('data-event-id')))
    assert.deepEqual(evidence.receipts, ['visual-action-1', 'visual-action-2', 'visual-action-3'])
    evidence.result = 'PASS healthy real buy completes; real renderer wrapper fails during sell; fallback idle retains all three confirmed fixture records'
    console.log(`PASS real stream ${width}: healthy buy, mid-sell failure, fallback with new transcript event`)
  } finally { await close(page, evidence) }
}
async function legacy(width) {
  const { page, evidence } = await open('legacy', width)
  try {
    await page.waitForSelector('#visual-status[data-loaded="true"]')
    await page.evaluate(actions => window.__visualState.updateSource('', actions), [arenaEvent(1, 'buy')])
    await page.waitForSelector('#visual-status[data-clip="act"]')
    await frame(page, evidence, 'a-act')
    await page.waitForFunction(() => window.__visualState.sceneLog.some(e => e.clip === 'accepted'))
    await page.waitForSelector('#visual-status[data-clip="idle"]')
    await page.evaluate(actions => window.__visualState.updateSource('', actions), [arenaEvent(1, 'buy'), arenaEvent(2, 'sell'), arenaEvent(3, 'produce')])
    await page.waitForSelector('#visual-status[data-clip="act"]')
    await page.evaluate(() => { window.__visualState.switchSource('/source-b'); window.__visualState.sceneLog.length = 0 })
    await page.waitForSelector('#visual-status[data-source="/source-b"][data-clip="idle"]')
    assert.equal(await page.locator('#visual-speech').textContent(), 'No queued speech')
    await frame(page, evidence, 'b-boundary-idle')
    await page.waitForTimeout(4500)
    assert.equal(await page.locator('#visual-status').getAttribute('data-clip'), 'idle')
    assert.equal(await page.locator('#visual-speech').textContent(), 'No queued speech')
    await page.evaluate(actions => window.__visualState.updateSource('/source-b', actions), [arenaEvent(1, 'vote')])
    await page.waitForSelector('#visual-status[data-clip="act"]')
    await page.waitForFunction(() => window.__visualState.sceneLog.some(e => e.clip === 'accepted'))
    await frame(page, evidence, 'b-result')
    assert((await page.locator('#visual-speech').textContent()).includes('Source B actor'))
    await page.waitForSelector('#visual-status[data-clip="idle"]')
    evidence.trace = await page.evaluate(() => window.__visualState.sceneLog)
    assert(!evidence.trace.some(e => e.speech.includes('Source A actor')))
    evidence.result = 'PASS real legacy act/result/idle sequence, same-game API-source switch cancels A work, fresh B vote plays and returns idle'
    console.log(`PASS real legacy ${width}: healthy act/result, clean source boundary, fresh B playback`)
  } finally { await close(page, evidence) }
}
try {
  await mkdir(output, { recursive: true })
  await build(config)
  server = await preview(config)
  browser = await chromium.launch({ channel: 'chrome', headless: true, timeout: 30_000 })
  observations.runtime = { node: process.version, chrome: browser.version(), fixtureOrigin: server.resolvedUrls.local[0], realTimers: true }
  for (const width of [1366, 390]) { await stream(width); await legacy(width) }
  console.log('PASS real-renderer visual fixture checks; inspect captured frames before visual acceptance.')
} finally {
  await writeFile(path.join(output, 'observations.json'), JSON.stringify(observations, null, 2) + '\n')
  await browser?.close(); await server?.close()
}
