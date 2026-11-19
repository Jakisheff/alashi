// Node-инициализация WASM-канона для чеков (node --experimental-strip-types).
// Тот же bundler-артефакт, что идёт в браузер: init принимает байты модуля,
// поэтому fetch не нужен. Вызывать до первого обращения к canon.ts.
import { readFileSync } from 'node:fs'
import init from '../src/rules-wasm/alashi_rules_wasm.js'

let ready: Promise<unknown> | null = null

export function initRulesNode(): Promise<unknown> {
  return (ready ??= init({ module_or_path: readFileSync(new URL('../src/rules-wasm/alashi_rules_wasm_bg.wasm', import.meta.url)) }))
}
