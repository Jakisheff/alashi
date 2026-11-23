// Гейт свежести WASM-канона правил: ожидания читаются из rules/src/constants.rs
// (текущий исходник), фактические значения — из закоммиченного WASM-артефакта
// (src/rules-wasm). Изменили rules и не перегенерировали артефакт — чек падает.
// Запуск: node --experimental-strip-types --no-warnings scripts/rules-wasm-check.ts
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { initRulesNode } from './nodeRulesInit.ts'
import {
  donkeyTerms, donkeyLimits, lawWireName, rulesVersion, totalRounds, lawDisplayForWire,
} from '../src/rules/canon.ts'

const constantsRs = readFileSync(new URL('../../rules/src/constants.rs', import.meta.url), 'utf8')

// --- Мини-вычислитель pub const выражений из constants.rs ---
function constExpr(raw: string): string | undefined {
  const m = constantsRs.match(new RegExp(`pub const ${raw}: \\w+ = ([^;]+);`))
  return m?.[1]?.trim()
}
function resolve(expr: string): number {
  const scope: Record<string, number> = {}
  for (const m of constantsRs.matchAll(/pub const ([A-Z_0-9]+): \w+ = ([^;]+);/g)) scope[m[1]] = m[2]
  const value = evalExpr(expr, scope)
  assert.ok(value !== undefined, `cannot resolve const expression: ${expr}`)
  return value as number
}
function evalExpr(expr: string, scope: Record<string, number | string>): number | undefined {
  const tokens = expr.match(/[A-Za-z_0-9]+|\d+|[+*/()-]/g) ?? []
  let i = 0
  const peek = () => tokens[i]
  const primary = (): number | undefined => {
    const t = tokens[i++]
    if (t === undefined || t.trim() === '') return undefined
    if (/^\d[\d_]*$/.test(t)) return Number(t.replaceAll('_', ''))
    if (t === '(') { const v = sum(); if (tokens[i++] !== ')') return undefined; return v }
    if (/^[A-Z_0-9]+$/.test(t)) { const s = scope[t]; if (typeof s === 'number') return s; if (s !== undefined) return evalExpr(String(s), scope) }
    return undefined
  }
  const term = (): number | undefined => { let v = primary(); while (v !== undefined && peek() === '*') { i++; const r = primary(); if (r === undefined) return undefined; v *= r } return v }
  const sum = (): number | undefined => { let v = term(); while (v !== undefined && (peek() === '+' || peek() === '-')) { const op = tokens[i++]; const r = term(); if (r === undefined) return undefined; v = op === '+' ? v + r : v - r } return v }
  const v = sum()
  return v !== undefined && i >= tokens.length ? v : undefined
}

const ROUNDS = resolve(constExpr('ROUNDS')!)
const FEE = resolve(constExpr('DONKEY_RISK_FEE')!)
const REWARD = resolve(constExpr('DONKEY_RISK_REWARD')!)
const SUCCESS = resolve(constExpr('DONKEY_RISK_SUCCESS_COUNT')!)
const OUTCOMES = resolve(constExpr('DONKEY_RISK_OUTCOMES')!)
const RULES_VERSION = resolve(constExpr('RULES_VERSION')!)

// --- Ожидаемые wire-имена карт: парсим сам match-блок law_wire_name ---
const matchBlock = constantsRs.match(/pub fn law_wire_name\(card: u8\) -> &'static str \{([\s\S]*?)\n\}/)?.[1] ?? ''
assert.ok(matchBlock.includes('=>'), 'law_wire_name match block not found in constants.rs')
const expectedWire = new Map<number, string>()
for (const m of matchBlock.matchAll(/(LAW_[A-Z_0-9]+|\d+) => "([a-z_0-9]+)"/g)) {
  const id = /^\d+$/.test(m[1]) ? Number(m[1]) : resolve(m[1])
  expectedWire.set(id, m[2])
}
assert.ok(expectedWire.size >= 9, `expected at least 9 named law cards, parsed ${expectedWire.size}`)

await initRulesNode()

// --- Сверка WASM со свежим исходником правил ---
assert.equal(rulesVersion(), RULES_VERSION, 'rules_version: WASM artifact is stale, rerun tools/build_rules_wasm.sh')
for (const [card, wire] of expectedWire) {
  assert.equal(lawWireName(card), wire, `law_wire_name(${card}): WASM artifact is stale, rerun tools/build_rules_wasm.sh`)
}
assert.equal(lawWireName(255), 'no_law')
assert.equal(lawWireName(200), 'no_law')

const terms = donkeyTerms()
assert.equal(terms.feeUnits, String(FEE), 'donkey fee: WASM artifact is stale, rerun tools/build_rules_wasm.sh')
assert.equal(terms.goods, REWARD)
assert.equal(terms.successPercent, Math.trunc((SUCCESS * 100) / OUTCOMES))
assert.equal(terms.successPercent + terms.emptyPercent, 100)

const limits = donkeyLimits()
assert.equal(limits.minCashUnits, String(FEE))
assert.equal(limits.maxGoodsBeforeAttempt, 65535 - REWARD)
assert.equal(limits.lastPlayableRound, ROUNDS - 1)
assert.equal(totalRounds(), ROUNDS)

// --- Display-мапа фронта обязана покрывать все wire-имена канона ---
for (const wire of expectedWire.values()) {
  const display = lawDisplayForWire(wire)
  assert.notEqual(display, wire.replaceAll('_', ' '), `frontend LAW_DISPLAY is missing wire name "${wire}"`)
}

console.log(`rules-wasm-check: ${expectedWire.size} law cards, fee ${terms.feeUnits}, success ${terms.successPercent}%, rounds ${totalRounds()} — WASM matches rules/src/constants.rs`)
