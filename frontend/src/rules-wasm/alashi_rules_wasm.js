/* @ts-self-types="./alashi_rules_wasm.d.ts" */

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
function __wbg_get_imports() {
    const import0 = {
        __proto__: null,
        __wbindgen_init_externref_table: function() {
            const table = wasm.__wbindgen_externrefs;
            table.set(0, undefined);
            table.set(1024, undefined);
            table.set(1025, null);
            table.set(1026, true);
            table.set(1027, false);
        },
    };
    return {
        __proto__: null,
        "./alashi_rules_wasm_bg.js": import0,
    };
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

let wasmModule, wasmInstance, wasm;
function __wbg_finalize_init(instance, module) {
    wasmInstance = instance;
    wasm = instance.exports;
    wasmModule = module;
    cachedUint8ArrayMemory0 = null;
    wasm.__wbindgen_start();
    return wasm;
}

async function __wbg_load(module, imports) {
    if (typeof Response === 'function' && module instanceof Response) {
        if (!module.ok) {
            throw new Error(`failed to fetch Wasm: ${module.status} ${module.statusText} fetching '${module.url}'`);
        }

        if (typeof WebAssembly.instantiateStreaming === 'function') {
            try {
                return await WebAssembly.instantiateStreaming(module, imports);
            } catch (e) {
                const validResponse = expectedResponseType(module.type);

                if (validResponse && module.headers.get('Content-Type') !== 'application/wasm') {
                    console.warn("`WebAssembly.instantiateStreaming` failed because your server does not serve Wasm with `application/wasm` MIME type. Falling back to `WebAssembly.instantiate` which is slower. Original error:\n", e);

                } else { throw e; }
            }
        }

        const bytes = await module.arrayBuffer();
        return await WebAssembly.instantiate(bytes, imports);
    } else {
        const instance = await WebAssembly.instantiate(module, imports);

        if (instance instanceof WebAssembly.Instance) {
            return { instance, module };
        } else {
            return instance;
        }
    }

    function expectedResponseType(type) {
        switch (type) {
            case 'basic': case 'cors': case 'default': return true;
        }
        return false;
    }
}

function initSync(module) {
    if (wasm !== undefined) return wasm;


    if (module !== undefined) {
        if (Object.getPrototypeOf(module) === Object.prototype) {
            ({module} = module)
        } else {
            console.warn('using deprecated parameters for `initSync()`; pass a single object instead')
        }
    }

    const imports = __wbg_get_imports();
    if (!(module instanceof WebAssembly.Module)) {
        module = new WebAssembly.Module(module);
    }
    const instance = new WebAssembly.Instance(module, imports);
    return __wbg_finalize_init(instance, module);
}

let __wbg_init_promise;
async function __wbg_init(module_or_path) {
    if (wasm !== undefined) return wasm;
    if (__wbg_init_promise === undefined) {
        __wbg_init_promise = __wbg_init_once(module_or_path);
        __wbg_init_promise.catch(() => { __wbg_init_promise = undefined; });
    }
    return __wbg_init_promise;
}

async function __wbg_init_once(module_or_path) {

    if (module_or_path !== undefined) {
        if (Object.getPrototypeOf(module_or_path) === Object.prototype) {
            ({module_or_path} = module_or_path)
        } else {
            console.warn('using deprecated parameters for the initialization function; pass a single object instead')
        }
    }

    if (module_or_path === undefined) {
        module_or_path = new URL('alashi_rules_wasm_bg.wasm', import.meta.url);
    }
    const imports = __wbg_get_imports();

    if (typeof module_or_path === 'string' || (typeof Request === 'function' && module_or_path instanceof Request) || (typeof URL === 'function' && module_or_path instanceof URL)) {
        module_or_path = fetch(module_or_path);
    }

    const { instance, module } = await __wbg_load(await module_or_path, imports);

    return __wbg_finalize_init(instance, module);
}

export { initSync, __wbg_init as default };
