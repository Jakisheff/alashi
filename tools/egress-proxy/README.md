# alashi egress-proxy (Diff 2, Zero-Trust RFC v2)

Закрывает оговорку README «Model usage is owner-submitted and is not
independently attested by ALASHI» для прогонов, чьи вызовы провайдера идут
через прокси: ключи живут только здесь, каждый завершённый вызов получает
подписанную квитанцию InferenceReceipt и строку в JSONL-журнале.

## Запуск

```bash
ALASHI_EGRESS_SEED=<64 hex> \
ALASHI_EGRESS_ALLOWED_HOSTS=int.api.provider.net,api.other.net \
ALASHI_EGRESS_JOURNAL=/path/receipts.jsonl \
ALASHI_EGRESS_MAX_TOTAL_TOKENS=10000000 \
ALASHI_EGRESS_MAX_CALLS=2000 \
TEST_PROVIDER_KEY=sk-... \
node server.mjs
```

Переменные: `ALASHI_EGRESS_SEED` (обяз., Ed25519 seed квитанций),
`ALASHI_EGRESS_ALLOWED_HOSTS` (обяз., хосты `host` или `host:port` через
запятую; запросы к любому другому хосту — 403 disallowed_endpoint ДО
подстановки ключа: прокси не может быть превращён в оракул креденшелов;
служебные переменные `ALASHI_*` в X-Alashi-Key-Env отвергаются),
`ALASHI_EGRESS_JOURNAL` (JSONL, по умолчанию `./egress-receipts.jsonl`),
`ALASHI_EGRESS_MAX_TOTAL_TOKENS` / `ALASHI_EGRESS_MAX_CALLS` (0 = без
лимита; проверка и инкременты сериализованы одним конвейером — конкурентные
вызовы не обходят кап; превышение фиксируется по факту, следующий вызов
режется до провайдера), `ALASHI_EGRESS_BIND`/`ALASHI_EGRESS_PORT` (по
умолчанию 127.0.0.1:эферемерный), `ALASHI_EGRESS_TIMEOUT_MS` (по умолчанию
120000; драйвер в egress-режиме использует одноимённую переменную со своим
значением по умолчанию 60000). Ключи провайдеров — только в окружении
прокси; https обязателен вне loopback.

## Контракт

- `POST /v1/chat/completions`, заголовки `X-Alashi-Endpoint` (URL провайдера),
  `X-Alashi-Key-Env` (имя переменной с ключом в окружении прокси), тело —
  chat-completions payload без изменений. Ответ: тело провайдера +
  заголовок `X-Alashi-Receipt` (base64 JSON квитанции).
- `GET /pubkey` — сырой 32-байтный публичный ключ подписи.
- `GET /healthz` — liveness + счётчики бюджета.
- Ошибки: `bad_endpoint`, `bad_key_env`, `unknown_key_env` (до вызова
  провайдера), `budget_exceeded` (429, до вызова провайдера),
  `provider_unreachable` (502).

## Квитанция (InferenceReceipt)

`receipt_id, ts_ms, model_id, endpoint_host, request_sha256,
response_sha256, prompt_cache_prefix_sha256, usage{input_fresh,
input_cache_read, input_cache_write, output}, latency_ms, http_status,
proxy_sig` (Ed25519 поверх канонической JSON-строки). Отсутствующие в ответе
провайдера числа остаются `null`, а не превращаются в аттестованный ноль.
`prompt_cache_prefix_sha256` — хеш model + первого system-сообщения:
диалоги и события мира в append-only хвосте не должны его менять
(защита от cache-busting DoS).

## Проверка журнала (Outside Judge)

```bash
node verify.mjs receipts.jsonl --pubkey <64 hex>
```

Проверяет подпись каждой строки, отклоняет дубликаты receipt_id (реплей),
пустой журнал и записи с неподписанными лишними полями; считает
аттестованные токены. Exit 1 = журнал не принимается.

## Идемпотентность ретраев

Журнал квитанций — sqlite (receipts.jsonl.db, WAL). Квитанция и списание
бюджета коммитятся одной транзакцией; счётчики бюджета ПРОИЗВОДНЫ от БД
(переживают рестарт). Ретрай того же тела на тот же endpoint с тем же
key-env → 409 duplicate_request с сохранённой квитанцией, без повторного
вызова провайдера и без повторного списания. При сбое коммита
(500 receipt_write_failed) ничего не списано: строки в БД нет.

## Границы доверия (честная оговорка)

Подпись доказывает целостность и происхождение записи, а не независимость
оператора: прокси запускает тот же владелец, что и эксперимент. Значение
этой границы: (а) агент/драйвер/раннер не видят ключей провайдеров;
(б) usage фиксируется механизмом (подписанная квитанция несёт числа так, как их заявил провайдер: подпись аттестует происхождение заявления, а не измеренную истину) в journal;
(в) журнал проверяем постфактум одной командой. Полный zero-tress
(независимый оператор прокси, внешняя аттестация seed) — следующий этап
по RFC; seed и pubkey стоит публиковать вместе с пакетом эксперимента.

## Статус интеграции

Прокси подключён к modelProvider (strategy-run.mjs): при
`ALASHI_EGRESS_URL` драйверы экспериментов не держат ключей, квитанция
остаётся в локальном журнале драйвера полем `attestation` и намеренно НЕ
уходит в отчёт арены (закрытое множество полей схемы). Известные границы
(починенное отмечено): бюджетные счётчики сериализованы внутри процесса,
но рестарт прокси обнуляет их — журнал не восстанавливает лимит
(ограничение зафиксировано); usage без cache-breakdown считается целиком
fresh input.
