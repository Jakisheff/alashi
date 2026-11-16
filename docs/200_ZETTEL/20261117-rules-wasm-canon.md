---
id: 20261117-rules-wasm-canon
tags: [rules, frontend, wasm, single-source]
status: source_defined
---

# rules-wasm: единый источник правил для клиента (2026-11-17)

## Наблюдение, породившее решение

Фронтенд зеркалил правила руками: `playback.ts` держал массив имён карт
законoppel с комментарием «Card IDs mirror rules/src/constants.rs»,
`donkey/model.ts` хардкодил `feeUnits: '5000000'` и проценты 60/40, тогда
как в Rust вероятность вычисляется из `DONKEY_RISK_SUCCESS_COUNT/DONKEY_RISK_OUTCOMES`
(3/5). Пороги eligibility (последний раунд, переполнение товара) были
числовыми литералами. Класс ошибки: правила меняются в Rust, зеркало на TS
забывается; зритель видит неверную карту или неверные условия.

## Решение

Крейт `rules-wasm` (собственный workspace по прецеденту companion — корневой
Cargo.toml остаётся чисто on-chain, SBF-сборка не видит wasm-зависимостей)
генерирует WASM-артефакт 22.7 KB (`tools/build_rules_wasm.sh`,
wasm-bindgen 0.2.130, web-target). Один артефакт обслуживает браузер
(`init()` по URL из Vite asset) и node-чеки (`scripts/nodeRulesInit.ts`
скармливает `init` байты файла — fetch не нужен).

Граница переноса: семантика да (wire-имена карт, donkey-условия, пороги,
эпохи, RULES_VERSION), презентация нет (display-строки во фронте, но
привязаны к wire-ключам, не к числовым id). Агрегация данных (standings,
победитель по payout-событиям) осталась в TS: она читает данные, а не
дублирует константы правил.

`law_wire_name` перенесён из `arena/src/api.rs` в `rules/src/constants.rs`:
арена ретранслирует канон, а не держит второй список.

## Гейт свежести

`frontend/scripts/rules-wasm-check.ts` парсит текущий `rules/src/constants.rs`
(константы-выражения и match-блок `law_wire_name`) и сверяет со значениями
из закоммиченного WASM. Изменить правила и забыть перегенерировать артефакт
= красный чек в CI (`npm run check:rules-wasm`). Проверен в обе стороны.
Выбран поведенческий гейт вместо sha256-манифеста по прецеденту sbf_gate:
он проверяет соответствие исходнику, а не только нетронутость файла.

## Ограничения

- Версия правил (`RULES_VERSION`) зашита в артефакт, но записи игр пока не
  несут версию правил: клиент не может отказать в интерпретации событий
  чужой версии. Требует серверной части (отдельная задача).
- Дифференциальный корпус: записанных epoch-2 игр нет; equivalence покрыта
  фикстурами adapter-check (91) и donkey-ui-check (98) плюс инвариантами
  в `rules/tests/rules_canon.rs` (полный перебор price_at и law_wire_name).

## Связи

- MOC-Specs (карта спек и канонов правил)
- [[20261026-client-scenario-implementation]] (граница клиент/сценарий)
- tools/build_rules_wasm.sh, frontend/src/rules/canon.ts
