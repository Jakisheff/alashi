import assert from 'node:assert/strict'
import { createServer } from 'node:http'
import { readFile, mkdir, writeFile } from 'node:fs/promises'
import path from 'node:path'
import { createHash } from 'node:crypto'
import { execFileSync } from 'node:child_process'
import { chromium } from '/Users/amir/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright/index.mjs'
const root = '/tmp/alashi-coin-dance-review/frontend/dist'
const out = '/tmp/alashi-coin-dance-evidence/phone-dance'
const mime = { '.js': 'text/javascript', '.css': 'text/css', '.html': 'text/html', '.png': 'image/png', '.svg': 'image/svg+xml', '.jpg': 'image/jpeg', '.woff2': 'font/woff2', '.json': 'application/json' }
const server = createServer(async (req, res) => {
  try {
    const url = new URL(req.url, 'http://localhost')
    const requested = url.pathname === '/live' || url.pathname === '/' ? '/index.html' : url.pathname
    const file = path.resolve(root, '.' + requested)
    if (!file.startsWith(root + '/')) { res.writeHead(403).end(); return }
    let data = await readFile(file)
    if (file.endsWith('/index.html')) data = Buffer.from(data.toString().replaceAll('https://alashi.network/', '/'))
    res.writeHead(200, { 'content-type': mime[path.extname(file)] ?? 'application/octet-stream' }); res.end(data)
  } catch { res.writeHead(404).end('Not found') }
})
await new Promise(resolve => server.listen(0, '127.0.0.1', resolve))
const origin = `http://127.0.0.1:${server.address().port}`
const observations = { source: 'c0e74cd8fc3eb2c3e531606dd2e24aa353af64ad', builtFrom: '026dabe435ad82c179fde8abf0e0c7089c57e264', runtimeDifference: 'none; final commit only adds two coin test assertions', origin, htmlAssetOrigins: 'absolute alashi.network icons localized to this static preview; JS unchanged', pages: [] }
let browser
function pixelDifference(a, b) {
  return JSON.parse(execFileSync('python3', ['-c', `from PIL import Image,ImageChops
import sys,json
a=Image.open(sys.argv[1]).convert("RGB");b=Image.open(sys.argv[2]).convert("RGB")
assert a.size==b.size
values=[max(p) for p in ImageChops.difference(a,b).get_flattened_data()]
print(json.dumps({"max_channel_delta":max(values),"pixels_above_8":sum(v>8 for v in values),"pixel_count":len(values)}))`, a, b], { encoding: 'utf8' }))
}
const digest = value => createHash('sha256').update(value).digest('hex')
async function seek(page, label, time) {
  const input = page.getByRole('slider', { name: label, exact: true })
  await input.evaluate((el, v) => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(el, String(v))
    el.dispatchEvent(new Event('input', { bubbles: true })); el.dispatchEvent(new Event('change', { bubbles: true }))
  }, time)
  await page.waitForTimeout(250)
  assert.equal(Number(await input.inputValue()), time)
}
async function capture(page, evidence, name) {
  const file = `${evidence.width}-${name}.png`
  await page.locator('.live-hero').scrollIntoViewIfNeeded()
  await page.waitForTimeout(150)
  await page.locator('.live-frame').screenshot({ path: path.join(out, file) })
  evidence.frames.push(file)
  console.log(`FRAME ${file}`)
}
try {
  browser = await chromium.launch({ channel: 'chrome', headless: true })
  await mkdir(out, { recursive: true })
  for (const width of [390]) {
    const context = await browser.newContext({ viewport: { width, height: 900 }, serviceWorkers: 'block', deviceScaleFactor: 1, recordVideo: { dir: path.join(out, '.capture'), size: { width, height: 900 } } })
    const page = await context.newPage(); page.setDefaultTimeout(30000)
    const evidence = { width, errors: [], consoleErrors: [], externalRequests: [], frames: [], checks: [] }
    observations.pages.push(evidence)
    page.on('pageerror', error => evidence.errors.push(error.message))
    page.on('console', message => { if (message.type() === 'error') evidence.consoleErrors.push(message.text()) })
    await page.route('**/*', route => {
      const url = new URL(route.request().url())
      if (url.origin === origin) return route.continue()
      evidence.externalRequests.push(url.href); return route.abort('blockedbyclient')
    })
    try {
      await page.goto(origin + '/live', { waitUntil: 'networkidle' })
      await page.locator('canvas').waitFor()
      await page.getByRole('button', { name: 'Pause demo', exact: true }).click()
      await page.locator('.live-hero').scrollIntoViewIfNeeded()
      await page.waitForTimeout(1800)
      evidence.gpu = await page.locator('canvas').evaluate(canvas => {
        const gl = canvas.getContext('webgl2') ?? canvas.getContext('webgl'); const ext = gl?.getExtension('WEBGL_debug_renderer_info')
        return gl && { version: gl.getParameter(gl.VERSION), renderer: ext ? gl.getParameter(ext.UNMASKED_RENDERER_WEBGL) : gl.getParameter(gl.RENDERER) }
      })
      assert(evidence.gpu)
      assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true)
      for (const [action, time] of []) {
        await page.getByRole('button', { name: `▶ ${action}`, exact: true }).click()
        await page.getByRole('slider', { name: 'Trade frame', exact: true }).waitFor()
        await seek(page, 'Trade frame', time)
        await capture(page, evidence, `coin-${action.toLowerCase()}-${time}`)
        const before = await page.getByRole('slider', { name: 'Trade frame', exact: true }).inputValue()
        await page.waitForTimeout(350)
        assert.equal(await page.getByRole('slider', { name: 'Trade frame', exact: true }).inputValue(), before)
        await seek(page, 'Trade frame', .45)
        await page.getByRole('button', { name: 'Resume trade animation', exact: true }).click()
        await page.locator('.live-hero').scrollIntoViewIfNeeded()
        await page.waitForSelector('.live-hero[data-action="idle"]', { timeout: 15000 })
        evidence.checks.push(`${action}: payment pose, pause, backward seek and full completion`)
      }
      await page.getByRole('button', { name: 'Thinking', exact: true }).click()
      await page.waitForTimeout(300)
      await page.getByRole('button', { name: 'Dance', exact: true }).click()
      await page.waitForSelector('.live-hero[data-reaction="dance"]')
      await page.locator('.live-hero').scrollIntoViewIfNeeded()
      evidence.visibleHeroAtDanceStart = await page.locator('.live-hero').boundingBox()
      await page.waitForFunction(() => Number(document.querySelector('input[aria-label="Reaction frame"]')?.value) > .15)
      for (const time of [0, .45, .9, 1.37, 2.8, 4.1]) {
        await seek(page, 'Reaction frame', time)
        await capture(page, evidence, `dance-${time}`)
        if (time === 0 || time === .9) await page.locator('canvas').screenshot({ path: path.join(out, `${width}-dance-positive-${time}.png`) })
      }
      evidence.normalSeekDifference = pixelDifference(path.join(out, `${width}-dance-positive-0.png`), path.join(out, `${width}-dance-positive-0.9.png`))
      assert(evidence.normalSeekDifference.pixels_above_8 > 100, 'Dance seek visibly changes rendered pose')
      await seek(page, 'Reaction frame', .45)
      const paused = await page.getByRole('slider', { name: 'Reaction frame', exact: true }).inputValue()
      await page.waitForTimeout(350)
      assert.equal(await page.getByRole('slider', { name: 'Reaction frame', exact: true }).inputValue(), paused)
      await page.getByRole('button', { name: 'Dance', exact: true }).click()
      const restarted = Number(await page.getByRole('slider', { name: 'Reaction frame', exact: true }).inputValue())
      assert(restarted < .5, `restart begins at ${restarted}`)
      await page.locator('.live-hero').scrollIntoViewIfNeeded()
      await page.waitForSelector('.live-hero[data-reaction="idle"]', { timeout: 12000 })
      evidence.checks.push('Dance: Thinking transition, key poses, pause/backward seek, restart and full completion')
      await page.emulateMedia({ reducedMotion: 'reduce' })
      await page.getByRole('button', { name: 'Dance', exact: true }).click()
      await page.locator('.live-hero').scrollIntoViewIfNeeded()
      await page.waitForTimeout(500)
      const aPath = path.join(out, `${width}-dance-reduced-a.png`)
      const a = await page.locator('canvas').screenshot({ path: aPath })
      await page.getByRole('button', { name: 'Pause reaction', exact: true }).click()
      evidence.reducedPauseTime = Number(await page.getByRole('slider', { name: 'Reaction frame', exact: true }).inputValue())
      await page.locator('.live-hero').scrollIntoViewIfNeeded()
      await page.waitForTimeout(350)
      const bPath = path.join(out, `${width}-dance-reduced-b.png`)
      const b = await page.locator('canvas').screenshot({ path: bPath })
      evidence.reducedCanvasHashes = [digest(a), digest(b)]
      evidence.reducedCanvasDifference = pixelDifference(aPath, bPath)
      assert(evidence.reducedCanvasDifference.max_channel_delta <= 4, 'Reduced-motion rendered pose differs by no more than4/255 GPU quantization')
      await capture(page, evidence, 'dance-reduced-motion')
      await seek(page, 'Reaction frame', 0)
      const zeroPath = path.join(out, `${width}-dance-reduced-seek-zero.png`)
      await page.locator('canvas').screenshot({ path: zeroPath })
      await seek(page, 'Reaction frame', 1.37)
      const selectedPath = path.join(out, `${width}-dance-reduced-seek-selected.png`)
      await page.locator('canvas').screenshot({ path: selectedPath })
      evidence.reducedSeekDifference = pixelDifference(zeroPath, selectedPath)
      assert(evidence.reducedSeekDifference.pixels_above_8 > 100, 'Reduced Dance explicit seek visibly changes pose')
      await seek(page, 'Reaction frame', .45)
      await page.getByRole('button', { name: 'Resume reaction', exact: true }).click()
      await page.locator('.live-hero').scrollIntoViewIfNeeded()
      await page.waitForSelector('.live-hero[data-reaction="idle"]', { timeout: 12000 })
      evidence.checks.push('Dance reduced motion: pause preserves displayed pose within4/255 raster tolerance, seek0/forward/backward visibly works, resume returns idle')
      assert.deepEqual(evidence.errors, [])
      assert.deepEqual(evidence.consoleErrors, [])
      assert.deepEqual(evidence.externalRequests, [])
      evidence.result = 'PASS'
      console.log(`PASS ${width}px: ${evidence.checks.join('; ')}`)
    } catch (error) {
      evidence.failure = error.message; console.error(`FAIL ${width}px: ${error.message}`); throw error
    } finally {
      await writeFile(path.join(out, 'observations.json'), JSON.stringify(observations, null, 2) + '\n')
      const video = page.video(); await context.close()
      evidence.video = `${width}-coin-dance.webm`; await video.saveAs(path.join(out, evidence.video)); await video.delete()
      await writeFile(path.join(out, 'observations.json'), JSON.stringify(observations, null, 2) + '\n')
    }
  }
} finally { await browser?.close(); await new Promise(resolve => server.close(resolve)) }
