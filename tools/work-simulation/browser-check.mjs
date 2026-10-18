// Optional local UI acceptance. Point ALASHI_TEST_PLAYWRIGHT_MODULE at an installed Playwright module.
import assert from 'node:assert/strict';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
const { chromium } = await import(process.env.ALASHI_TEST_PLAYWRIGHT_MODULE || 'playwright');
const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const out = resolve(process.env.ALASHI_TEST_BROWSER_OUTPUT || resolve(root, 'experiments/work-simulation/evidence'));
mkdirSync(out, { recursive: true });
const browser = await chromium.launch({ channel: 'chrome', headless: true });
const errors = [];
try {
  const page = await browser.newPage({ viewport: { width: 1440, height: 1100 } });
  page.on('pageerror', error => errors.push(String(error)));
  await page.goto('http://127.0.0.1:5183/work-simulation', { waitUntil: 'networkidle' });
  await page.getByRole('heading', { name: 'Произвести товар. Довести его до казны.' }).waitFor();
  assert.equal(await page.locator('.work-cell').count(), 4);
  assert.ok((await page.locator('.work-test-banner').innerText()).includes('Rust'));
  await page.locator('.work-cell').first().click();
  await page.getByRole('heading', { name: 'Партии: конфигурация A' }).waitFor();
  await page.getByRole('button', { name: '29 · 3', exact: true }).click();
  await page.getByText('Квитанции движка: seed 29, место 3', { exact: true }).waitFor();
  const pending = page.waitForResponse(r => r.url().endsWith('/work-simulation-api/run') && r.request().method() === 'POST');
  await page.getByRole('button', { name: 'Повторить TEST-прогон' }).click();
  const response = await pending; assert.equal(response.status(), 200);
  const report = await response.json();
  assert.equal(report.schema, 'alashi.work-simulation.TEST.v2');
  assert.deepEqual(report.runs.map(r => r.games.length), [12, 12, 12, 12]);
  for (const [path, expected] of Object.entries(report.sources)) {
    assert.equal(createHash('sha256').update(readFileSync(resolve(root, 'tools/work-simulation', path))).digest('hex'), expected);
  }
  assert.ok(report.runs.every(r => r.games.every(g => g.produced === 5 && g.sales === 5 && g.invalid_actions === 0)));
  await page.getByRole('button', { name: 'Повторить TEST-прогон' }).waitFor();
  await page.getByRole('heading', { name: 'Партии: конфигурация A' }).waitFor();
  await page.getByText('Квитанции движка: seed 29, место 3', { exact: true }).waitFor();
  const download = page.waitForEvent('download');
  await page.getByRole('link', { name: 'Скачать результат с основаниями ↓' }).click();
  const receiptPath = resolve(out, 'browser-result.json');
  await (await download).saveAs(receiptPath);
  const receipt = JSON.parse(readFileSync(receiptPath));
  assert.equal(receipt.native_binary_sha256, report.native_binary_sha256);
  assert.equal(receipt.knowledge_probe.hypothesis_status, 'not_validated');
  await page.locator('.work-cell').last().click();
  await page.getByRole('button', { name: '11 · 1', exact: true }).click();
  await page.evaluate(() => scrollTo(0, 0));
  await page.screenshot({ path: resolve(out, 'desktop.png') });
  await page.getByText('Квитанции движка: seed 11, место 1', { exact: true }).scrollIntoViewIfNeeded();
  await page.screenshot({ path: resolve(out, 'receipts.png') });
  await page.getByRole('heading', { name: '«Биологический ров» и то, что не сказано вслух' }).scrollIntoViewIfNeeded();
  await page.screenshot({ path: resolve(out, 'knowledge.png') });
  await page.setViewportSize({ width: 390, height: 844 });
  await page.evaluate(() => scrollTo(0, 0));
  assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
  await page.screenshot({ path: resolve(out, 'mobile.png') });
  await page.getByRole('heading', { name: 'Профиль для проверки' }).scrollIntoViewIfNeeded();
  await page.screenshot({ path: resolve(out, 'mobile-profile.png') });
  const beforeError = await page.locator('.work-cell strong').allTextContents();
  await page.route('**/work-simulation-api/run', route => route.fulfill({ status: 503, contentType: 'application/json', body: '{"error":"TEST_failure"}' }));
  await page.getByRole('button', { name: 'Повторить TEST-прогон' }).click();
  await page.getByRole('alert').waitFor();
  assert.deepEqual(await page.locator('.work-cell strong').allTextContents(), beforeError);
  assert.deepEqual(errors, []);
  writeFileSync(resolve(out, 'ui-copy.txt'), await page.locator('main').innerText());
  const summary = { browser: 'isolated headless Chrome', desktop: [1440, 1100], mobile: [390, 844],
    page_errors: errors, configuration_selection: true, game_selection: true, real_local_rerun: true,
    native_games: 48, source_hashes_match: true, download_receipt: true, error_preserves_previous_results: true,
    mobile_no_horizontal_overflow: true, human_data: false };
  writeFileSync(resolve(out, 'browser-check.json'), JSON.stringify(summary, null, 2) + '\n');
  console.log(JSON.stringify(summary));
} finally { await browser.close(); }
