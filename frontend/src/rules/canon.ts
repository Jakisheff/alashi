// Канонические игровые термины из одного источника: WASM-сборка крейта
// alashi-rules (frontend/src/rules-wasm, генерируется tools/build_rules_wasm.sh
// из ../rules-wasm). display-тексты живут здесь и привязаны к wire-ключам,
// не к индексам карт: числовые id может знать только rules-крейт.
import {
  donkey_empty_percent,
  donkey_fee_units,
  donkey_last_playable_round,
  donkey_max_goods_before_attempt,
  donkey_reward_goods,
  donkey_success_percent,
  law_wire_name,
  rules_version,
  total_rounds,
} from '../rules-wasm/alashi_rules_wasm.js'

/**
 * WASM-модуль обязан быть инициализирован до первого вызова функций канона.
 * Браузер: src/rules/browserInit.ts (вызывается из main.tsx до рендера).
 * Node-чеки: scripts/nodeRulesInit.ts. Здесь только чистие обёртки.
 */

export function rulesVersion(): number {
  return rules_version()
}

export function lawWireName(card: number): string {
  return law_wire_name(card)
}

export function donkeyTerms(): { feeUnits: string; goods: number; successPercent: number; emptyPercent: number } {
  return {
    feeUnits: donkey_fee_units(),
    goods: donkey_reward_goods(),
    successPercent: donkey_success_percent(),
    emptyPercent: donkey_empty_percent(),
  }
}

export function donkeyLimits(): { minCashUnits: string; maxGoodsBeforeAttempt: number; lastPlayableRound: number } {
  return {
    minCashUnits: donkey_fee_units(),
    maxGoodsBeforeAttempt: donkey_max_goods_before_attempt(),
    lastPlayableRound: donkey_last_playable_round(),
  }
}

export function totalRounds(): number {
  return total_rounds()
}

// Display-имена: ключи обязаны совпадать с wire-именами rules-крейта
// (alashi_rules::constants::law_wire_name); проверяется rules-wasm-check.
const LAW_DISPLAY: Record<string, string> = {
  status_quo: 'Status quo',
  tax_10: 'Tax 10%',
  tax_20: 'Tax 20%',
  subsidy_produce: 'Production subsidy',
  subsidy_poor: 'Subsidy for the poorest',
  subsidy_rich: 'Subsidy for the richest',
  embargo: 'Embargo',
  boom: 'Boom',
  vzaimozachet: 'Mutual offset',
}

/** Человекочитаемое имя карты по числовому id (через wire-ключ канона). */
export function lawDisplayName(card: number | undefined): string {
  if (card === undefined) return 'card unavailable'
  const wire = lawWireName(card)
  return LAW_DISPLAY[wire] ?? `Card #${card}`
}

/** Display-имя для wire-ключа (для метаданных, пришедших уже строкой). */
export function lawDisplayForWire(wire: string): string {
  return LAW_DISPLAY[wire] ?? wire.replaceAll('_', ' ')
}
