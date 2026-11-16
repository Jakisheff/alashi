// Браузерная инициализация WASM-канона правил. Вызывается из main.tsx
// до первого рендера; сами функции канона живут в canon.ts.
import init from '../rules-wasm/alashi_rules_wasm.js'

let ready: Promise<unknown> | null = null

/** Инициализация WASM-модуля; идемпотентна. */
export function initRules(): Promise<unknown> {
  return (ready ??= init())
}
