/**
 * Процент пустых исходов: (OUTCOMES-SUCCESS_COUNT)/OUTCOMES.
 * @returns {number}
 */
export function donkey_empty_percent() {
    const ret = wasm.donkey_empty_percent();
    return ret;
}

/**
 * Плата за donkey-попытку в минорных единицах (строка: u64 вне JS safe integer).
 * @returns {string}
 */
export function donkey_fee_units() {
    let deferred1_0;
    let deferred1_1;
    try {
        const ret = wasm.donkey_fee_units();
        deferred1_0 = ret[0];
        deferred1_1 = ret[1];
        return getStringFromWasm0(ret[0], ret[1]);
    } finally {
        wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
    }
}

/**
 * Последний раунд donkey-попытки (в ROUNDS нет следующей Market-фазы).
 * @returns {number}
 */
export function donkey_last_playable_round() {
    const ret = wasm.donkey_last_playable_round();
    return ret;
}

/**
 * Максимальный запас товара, при котором попытка ещё не переполняет u16.
 * @returns {number}
 */
export function donkey_max_goods_before_attempt() {
    const ret = wasm.donkey_max_goods_before_attempt();
    return ret;
}

/**
 * Товаров при успехе donkey-попытки.
 * @returns {number}
 */
export function donkey_reward_goods() {
    const ret = wasm.donkey_reward_goods();
    return ret;
}

/**
 * Процент исходов с выигрышем: SUCCESS_COUNT/OUTCOMES, вычислен, не скопирован.
 * @returns {number}
 */
export function donkey_success_percent() {
    const ret = wasm.donkey_success_percent();
    return ret;
}

/**
 * Эпоха с donkey-лотереей (платная попытка, разрешение только из VRF).
 * @returns {number}
 */
export function epoch_donkey_risk() {
    const ret = wasm.epoch_donkey_risk();
    return ret;
}

/**
 * Правила экономики 90-х применяются к этой эпохе.
 * @param {number} epoch
 * @returns {boolean}
 */
export function has_90s_rules(epoch) {
    const ret = wasm.has_90s_rules(epoch);
    return ret !== 0;
}

/**
 * Wire-имя карты закона (канон `alashi_rules::constants::law_wire_name`).
 * @param {number} card
 * @returns {string}
 */
export function law_wire_name(card) {
    let deferred1_0;
    let deferred1_1;
    try {
        const ret = wasm.law_wire_name(card);
        deferred1_0 = ret[0];
        deferred1_1 = ret[1];
        return getStringFromWasm0(ret[0], ret[1]);
    } finally {
        wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
    }
}

/**
 * Версия правил, зашитая в этот артефакт. Клиент может показать её
 * или отказаться интерпретировать события чужой версии.
 * @returns {number}
 */
export function rules_version() {
    const ret = wasm.rules_version();
    return ret >>> 0;
}

/**
 * Всего раундов в партии (граница валидации номера раунда на клиенте).
 * @returns {number}
 */
export function total_rounds() {
    const ret = wasm.total_rounds();
    return ret;
}
export function __wbindgen_init_externref_table() {
    const table = wasm.__wbindgen_externrefs;
    table.set(0, undefined);
    table.set(1024, undefined);
    table.set(1025, null);
    table.set(1026, true);
    table.set(1027, false);
}
function getStringFromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return decodeText(getUint8ArrayMemory0().subarray(ptr, ptr + len));
}

let cachedUint8ArrayMemory0 = null;
function getUint8ArrayMemory0() {
    if (cachedUint8ArrayMemory0 === null || cachedUint8ArrayMemory0.byteLength === 0) {
        cachedUint8ArrayMemory0 = new Uint8Array(wasm.memory.buffer);
    }
    return cachedUint8ArrayMemory0;
}

let cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true });
cachedTextDecoder.decode();
const MAX_SAFARI_DECODE_BYTES = 2146435072;
let numBytesDecoded = 0;
function decodeText(bytes) {
    numBytesDecoded += bytes.length;
    if (numBytesDecoded >= MAX_SAFARI_DECODE_BYTES) {
        cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true });
        cachedTextDecoder.decode();
        numBytesDecoded = bytes.length;
    }
    return cachedTextDecoder.decode(bytes);
}


let wasm;
export function __wbg_set_wasm(val) {
    wasm = val;
}
