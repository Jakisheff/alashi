# HANDOFF — контекст для следующей сессии (01.09 12:05)

> status: archived (01.09 19:5x). Файл устарел: арена построена и
> работает, актуальный статус — PROGRESS.md и STATUS.md, разбор
> внешнего ревью — docs/REVIEW_EXTERNAL_2026-09-01.md.

## КРИТИЧНО: что только что решили

Владелец подтвердил стратегию: **off-chain арена = полноценный продукт ДО блокчейна**.
Это было изначальное видение владельца (training camp в GAME_BIBLE), потерянное
в спринте. Двухуровневая архитектура:

- v0: off-chain арена (симулятор + HTTP API + метрики) — бесплатно, без кошельков
- v1: on-chain слой (Solana + ставки + верификация) — поверх v0, для сертификации

## ЧТО В ПРОГРЕССЕ (прервано на середине)

Начал создавать crate `arena/` — HTTP API поверх `rules/src/sim.rs`:
- `arena/Cargo.toml` создан (зависимости: alashi-rules, serde, serde_json, sha2)
- `arena/src/` пуст — код НЕ написан
- НЕ закоммичен

## КАК ПОСТРОИТЬ ARENA (план из docs/SIM_TEST_PLAN.md + docs/OFFCHAIN_ARENA.md)

HTTP API поверх симулятора alashi-rules. Endpoints:

```
POST /game/new          — создать партию
POST /game/:id/join     — агент заходит (name, model, prompt) → agent_id + token
GET  /game/:id/state    — состояние (фаза, фракции, цены, закон)
POST /game/:id/act      — ход агента (token, action, params)
GET  /games             — активные партии
GET  /leaderboard       — рейтинги
GET  /export            — завершённые партии JSONL
```

Реализация:
1. Использовать `rules::sim::Simulator` напрямую
2. HashMap<u64, Simulator> для активных игр
3. Фоновый поток для кранка фаз (таймер)
4. Авторизация: agent_id = sha256(model|prompt), token = случайный hex
5. Деплой на Railway/Fly.io

Оценка: 6-7 часов на API + 16-21 час на 7 метрик (docs/SIM_TEST_PLAN.md)

## СОСТОЯНИЕ ПРОЕКТА на 01.09 12:05

- До freeze (03.09 18:00): ~54 часа
- До демо (04.09): ~69 часов
- Код: 25 тестов зелёные, join-режим работает, 13 инструкций
- Devnet: ПОКА ПУСТО (faucet rate-limited с 30.08) — см. docs/DEVNET_SOL_PLAN.md
- Репо: ПРИВАТНОЕ (нужно сделать публичным) — скан секретов пройден, чисто
- Ветка: sprint-31-08, последний коммит dd1f9f3

## КРИТИЧЕСКИЙ ПУТЬ ДО ДЕМО (по убыванию)

1. Devnet SOL (перевод от человека в чат Superteam KZ ИЛИ QuickNode faucet)
2. Публичный репо (GitHub settings → visibility)
3. Деплой на devnet + открытая партия
4. Три отправки: чат AI_PLAN, пост elpresidente, письмо Максиму
5. Репетиция ×3 (DEMO_SCRIPT.md готов, слайды DECK.md готовы)
6. OFF-CHAIN ARENA (если время останется после 1-5)

## ВАЖНЫЕ ПРАВИЛА (вшиты в AGENTS.md)

- Фильтр Морейниса: 6 принципов обязательны для каждой идеи
- Мини-отчёты каждые 10-15 минут
- План/Факт пары в PROGRESS.md
- Время: только TZ=Asia/Almaty date
- Мухтар исключён из всех раскладов владельцем

## КЛЮЧЕВЫЕ ДОКУМЕНТЫ (все в docs/)

| Файл | Что |
|---|---|
| DECK.md | 8 слайдов, готовых к репетиции (Zhambyl-открытие, killer feature 5/9) |
| DEMO_SCRIPT.md | 3 минуты посекундно, чек-лист |
| QA.md | 13 вопросов жюри с ответами |
| COMPETITOR_MATRIX.md | 23 соседа из 3 экосистем, ближайший Daemon Hall |
| STRATEGIC_VERDICT.md | Вердикт (б): валидна для демо, пересмотр после Colosseum |
| MOREINIS_PRODUCT_LOGIC.md | 6 принципов применены к alashi |
| METRICS_INTEL_PASS2.md | 2 killer features (память + экспроприация) |
| EVAL_METHODOLOGIES.md | 7 методологий (исследование владельца) |
| SIM_TEST_PLAN.md | Как проверить 7 методологий в симуляторе |
| ZHAMBYL_ATTRACT.md | Zhambyl = тест (не внешний), как привлечь настоящих |
| DEVNET_SOL_PLAN.md | Как решить проблему SOL (перевод/QuickNode) |
| PG_PRINCIPLES.md | Конспект Грэма своими словами |
| YC_DEEPDIVE.md | Olam Labs = ближайший YC-сосед |
| PROOFPILOT_VALIDATION.md | Валидация по методу Максима, вердикт apply_after_7_day_sprint |
| TAKEOUT_INSIGHTS.md | 34 инсайта из Google Docs владельца |
| AGENT_GUIDE.md | Как подключить агента (runbook) |
| TRIZ_ROUND2.md | Настоящие ТП + приоритизация Б1-Б7 |

## ЛИЧНЫЕ ФАЙЛЫ ВЛАДЕЛЬЦА (вне репо)

- ~/Downloads/amir/maxim_draft.md — черновик письма Максиму (ГОТОВ, не отправлен)
- ~/Downloads/amir/maxim_draft2.md — версия 2
- ~/Downloads/amir/maxim_chat_excerpt.md — выдержка переписки
- ~/Downloads/amir/temnaya_dump.txt — дамп канала Морейниса (94 поста)
- ~/Downloads/amir/research/glm/moreinis-product-logic.txt — транскрипция видео
- ~/Documents/pg_essays/ — 233 эссе Грэма + DIGEST.md + INDEX.md
- ~/Downloads/Takeout/ — выгрузка Google Docs

## ДАННЫЕ

- data/ethglobal_projects.jsonl: 222 проекта (ETHGlobal NYC+Lisbon 2026)
- data/yc_companies.jsonl: 582 компании (YC все батчи, agent×crypto)
- data/events_stream.jsonl: 207 событий из живых партий
- data/exports/parties.jsonl: 1 полная партия (obs/action/obs_after)
- data/registry.json: 3 зарегистрированных агента
- tools/ethglobal_crawl.py, tools/yc_crawl.py, tools/expro_metric.py

## ПОСЛЕДНИЕ КОММИТЫ (для ориентира)

```
dd1f9f3 SIM_TEST_PLAN: 7 методологий в симуляторе
75f36b0 ZHAMBYL_ATTRACT: честный разбор
d0c67ab AGENTS: фильтр Морейниса вшит
b967ee6 MOREINIS_ПРАВКИ: слайд 2 + QA №8
f0284a9 MOREINIS_PRODUCT_LOGIC: разбор видео
cb96fbc EVAL_METHODOLOGIES: 7 методологий
5dcb629 DECK_FINAL_PASS: слайды прорежены, дыры закрыты
1101fe5 PITCH_ZHAMBYL: DEMO_SCRIPT переписан
```
