/* tslint:disable */
/* eslint-disable */

/**
 * Процент пустых исходов: (OUTCOMES-SUCCESS_COUNT)/OUTCOMES.
 */
export function donkey_empty_percent(): number;

/**
 * Плата за donkey-попытку в минорных единицах (строка: u64 вне JS safe integer).
 */
export function donkey_fee_units(): string;

/**
 * Последний раунд donkey-попытки (в ROUNDS нет следующей Market-фазы).
 */
export function donkey_last_playable_round(): number;

/**
 * Максимальный запас товара, при котором попытка ещё не переполняет u16.
 */
export function donkey_max_goods_before_attempt(): number;

/**
 * Товаров при успехе donkey-попытки.
 */
export function donkey_reward_goods(): number;

/**
 * Процент исходов с выигрышем: SUCCESS_COUNT/OUTCOMES, вычислен, не скопирован.
 */
export function donkey_success_percent(): number;

/**
 * Эпоха с donkey-лотереей (платная попытка, разрешение только из VRF).
 */
export function epoch_donkey_risk(): number;

/**
 * Правила экономики 90-х применяются к этой эпохе.
 */
export function has_90s_rules(epoch: number): boolean;

/**
 * Wire-имя карты закона (канон `alashi_rules::constants::law_wire_name`).
 */
export function law_wire_name(card: number): string;

/**
 * Версия правил, зашитая в этот артефакт. Клиент может показать её
 * или отказаться интерпретировать события чужой версии.
 */
export function rules_version(): number;

/**
 * Всего раундов в партии (граница валидации номера раунда на клиенте).
 */
export function total_rounds(): number;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly donkey_empty_percent: () => number;
    readonly donkey_fee_units: () => [number, number];
    readonly donkey_last_playable_round: () => number;
    readonly donkey_max_goods_before_attempt: () => number;
    readonly donkey_reward_goods: () => number;
    readonly donkey_success_percent: () => number;
    readonly epoch_donkey_risk: () => number;
    readonly has_90s_rules: (a: number) => number;
    readonly law_wire_name: (a: number) => [number, number];
    readonly rules_version: () => number;
    readonly total_rounds: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
