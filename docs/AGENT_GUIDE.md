# AGENT_GUIDE, как подключить своего агента к партии Alashi

Runbook для любого агента или человека: подключиться к живой партии,
играть свои ходы, получить выплату. Никакой платформы не нужно: RPC, ключ,
транзакции. Проверено e2e 31.08: третья фракция (join-режим) сыграла полную
партию 6 раундов и выиграла её у обоих ботов хоста, выплата пришла на кошелёк.

## 0. Локальный JSON-клиент для OpenCode и других агентов (classic, devnet)

Новый режим `bots agent` отдаёт **один JSON в stdout**; код выхода `0` означает успешный
результат команды, ненулевой — ошибку. Он не вызывает LLM, не создаёт ключ, не запрашивает
airdrop и не хранит ключ на сервере. OpenCode выбирает действие, этот клиент локально
подписывает его. Проверенный модельный идентификатор оператора: `zai-coding-plan/glm-5.3-flash`;
модель не является частью протокола Alashi.

Команды выполняются **на компьютере владельца**, из корня репозитория. Перед запуском
подготовь отдельный devnet keypair с правами `600`, положи его вне репозитория и пополни
devnet SOL. Не передавай содержимое keypair в промпт, модель, логи или сервер. Значения
переменных ниже — локальные пути и публичные адреса; ключ не показывается:

```sh
export ALASHI_RPC=https://api.devnet.solana.com
ALASHI_GAME='<game public key from host>'
ALASHI_WALLET='<your public wallet address>'
ALASHI_DEVNET_KEYPAIR='/absolute/private/path/agent.json'

# Только чтение: state и твоя фракция; подпись не нужна.
cargo run --quiet --locked --manifest-path bots/Cargo.toml -- \
  agent inspect --game "$ALASHI_GAME" --wallet "$ALASHI_WALLET"

# Подписанный join; уже существующая фракция возвращает already_joined без оплаты.
cargo run --quiet --locked --manifest-path bots/Cargo.toml -- \
  agent join --game "$ALASHI_GAME" --name IvanAgent --key "$ALASHI_DEVNET_KEYPAIR"

# Ровно одно решение JSON через stdin. Сначала inspect и проверь фазу.
printf '%s\n' '{"action":"produce"}' | \
  cargo run --quiet --locked --manifest-path bots/Cargo.toml -- \
  agent act --game "$ALASHI_GAME" --key "$ALASHI_DEVNET_KEYPAIR"
```

Все три команды проверяют genesis devnet, owner, discriminator и PDA Game; фракции
также проверяются на owner/PDA/принадлежность игре. Используй доверенный RPC: genesis
защищает от случайного выбора другой сети, а не от лживого сервера. Game.account_lamports показывает баланс аккаунта вместе с резервом rent, а не только банк игры. Изменяющие команды
поддерживают `epoch=0` (classic). `inspect` возвращает `state`, `your_faction` (либо null),
`observed_slot` и `commitment: confirmed`. `available_action_types` учитывает фазу,
текущий stamp, живую фракцию и право вето; баланс, параметры и целевой аккаунт проверяет
программа. Список не обещает успех каждой операции. `cash` — игровые единицы, `1000000`
равно одному песо; price_table — целые песо за единицу товара; `entry_fee_lamports` и выплаты — lamports devnet SOL. Все `u64`
денежные значения и `game_id` в JSON представлены десятичными строками.

Действия:

```json
{"action":"sell","params":{"units":2}}
{"action":"buy","params":{"units":1}}
{"action":"produce"}
{"action":"donkey"}
{"action":"bribe","params":{"to":"<target faction pubkey>","amount":"5000000"}}
{"action":"vote","params":{"choice":"yes"}}
{"action":"veto"}
```

Каждая строка — отдельный вызов, а не один JSON-документ. `bribe.to` — **PDA фракции**,
не адрес кошелька. `name` ограничен 16 **UTF-8 байтами**, не 16 символами.

Подтверждённый ответ join/act имеет форму:

```json
{"ok":true,"command":"act","network":"devnet","program_id":"…","game":"…","wallet":"…","faction":"…","receipt":{"status":"confirmed","signature":"…","slot":123,"block_time":0,"log_messages":[]}}
```

Это пример схемы, не результат настоящей транзакции. `confirmed` появляется только
после `getTransaction(commitment=confirmed)` с `meta.err=null`. Значение `ok=true`
означает выполнение действия, не прибыль или победу. Повторный join своей существующей
фракции возвращает `status: already_joined`, `receipt: null` и не подписывает транзакцию.

Ошибка транзакции возвращает `ok: false`, ненулевой exit code и `receipt.status`:

- `not_sent`: блокхэш или подпись не подготовлены;
- `rejected`: известная ошибка preflight, операция не принята;
- `failed`: подтверждённая транзакция имеет `meta.err`;
- `unknown`: отправка/подтверждение неоднозначны; signature сохранена, если транзакция
  была подписана. Legacy host/guest останавливается с exit code 3 при unknown. **Не повторяй ход автоматически:** сначала проверь signature в devnet
  и заново прочитай state. Истёкший ответ не означает отказа цепи.

Ошибки ввода/сети/аккаунта возвращают `error: {code, message}` без содержимого ключа или
адреса RPC. Вывод Cargo идёт в stderr. В обёртке для модели разрешай только inspect,
join, act и ограниченное ожидание; сам путь keypair подставляет локальная обёртка.

Хост для агента, которому нужно время на решения (кошельки host остаются локально):

```sh
cargo run --quiet --locked --manifest-path /absolute/path/alashi/bots/Cargo.toml -- \
  --phase-duration 45 --timeout 2000 --no-llm
```

Рабочий каталог хоста должен быть отдельным приватным каталогом с `bots/keys/bot1.json`
и `bot2.json`; без них старый host создаёт кошельки и может запросить airdrop. Лобби длится
`5 × phase_duration`, затем 18 фаз. `--timeout` должен покрывать весь матч. Без флагов
прежние значения сохранены: 15 секунд/фаза, 720 секунд timeout; при увеличении длительности
фазы timeout по умолчанию увеличивается автоматически. `--no-llm` исключает чтение LLM
credentials хостом. Host открывает фракции гостей перед advance/settle. Внешний CLI играет
только своей фракцией; завершение игры обеспечивает host. Не менять program/rules ради
подключения агента.

## 1. Lock the target

- Программа: `3jwunaFDRrSWFfeJ5hFZu3DmxPNTmkdoCweHDvqcXTqC` (devnet, задеплоена 05.10.2026)
- RPC: любой devnet-эндпоинт (`https://api.devnet.solana.com` или платный)
- Вывод адресов:
  - game PDA = `["game", game_id_le]` от program id
  - faction PDA = `["faction", game, wallet]` от program id
- До любой транзакции сверь owner аккаунта Game с program id выше. Фейковая
  программа с такой же механикой это основной вектор обмана.

## 2. Classify: кто ты в партии

- Host (организатор): инициализировал игру через `initialize`, двигает
  фазы (`advance`) и делает `settle`. Обязан знать ВСЕ фракции партии
  (см. §5), иначе партия встанет.
- Guest (третья сторона): вступил через `join` в фазе Lobby, играет
  только свои ходы. Кранкать фазы не обязан и не может без полного списка
  фракций.
- Observer: только чтение аккаунтов, транзакций не нужно.

## 3. Машина фаз и тайминг

6 раундов × 3 фазы: Market → Action → Law. Длительность фазы задаёт host в
`initialize` (unix-дедлайны, поле `phase_ends_at`). Lobby живёт 5×фаза
(по умолчанию 75 с). Успей в окно: после дедлайна любой (`advance`) двинет
фазу дальше, опоздавший ход отклонится.

## 4. Action space (JSON-протокол)

| Фаза | Действие | Поля | Ограничения |
|---|---|---|---|
| Market | sell | units | units ≤ goods |
| Market | buy | units | cash ≥ units × цена |
| Action | produce |, | +2 товара |
| Action | bribe | amount (5 песо) | цель = faction PDA чужой фракции |
| Action | donkey |, | +1 товар за 1 песо |
| Law | vote | yes / no / abstain | вес = influence |
| Law | veto |, | только президент, до подсчёта |

Аккаунты каждой инструкции см. в reference-клиенте (`bots/src/main.rs`,
функции `ix_*`). Осёл и взятка существуют, чтобы выживать при жёстких законах: 8 карт
законов меняют налог, субсидии, цены.

## 5. Контракт host/guest (важно)

`advance` и `settle` требуют полный набор фракций партии
(`InvalidFactionSet` иначе). Хост, который знает только своих ботов,
блокирует партию после вступления гостя. Правильный хост переоткрывает
список фракций по цепи:

`getProgramAccounts(program, filters=[Memcmp(offset=8, game)], encoding="base64")`

Вызов с явным base64 проверен на публичном devnet RPC 05.10.2026; см.
[отчёт деплоя](ops/DEVNET_DEPLOY_20261005.md). Лимиты RPC всё равно нужно учитывать. Guest ничего этого не делает: он играет свои ходы и ждёт,
пока host двигает фазы. Settle выплатит на кошелёк каждой фракции
50/30/15/5 по рангу богатства, рейк 5% админу.

## 6. Быстрый старт готовым клиентом

```bash
cd bots
ALASHI_RPC=<rpc> cargo run --release -- \
  --game <GAME_PUBKEY> --name <ИМЯ_ДО_16_БАЙТ> [--key путь/к/ключу.json]
```

Ключ создастся сам (`bots/keys/join.json` по умолчанию), при нехватке
средств клиент попробует devnet-airdrop. Стратегия гостя: продать всё в
Market, produce в Action, голос NO в Law. Замена стратегии = правка одного
match-блока в `run_join_mode`.

## 7. Failure tree

| Симптом | Причина | Что делать |
|---|---|---|
| join отклонён | фаза уже не Lobby / партия полная (6 фракций) / мало lamports | ждите новую партию, пополните кошелёк |
| TooEarly на advance | дедлайн фазы не наступил | ждать, сверять `phase_ends_at` с chain time |
| InvalidFactionSet | неполный список фракций в advance/settle | открыть фракции по цепи (§5) |
| AlreadySettled / NotFinished | повторный settle / ранний settle | читать `game.settled`, `game.phase` |
| LawNotRevealed | партия в VRF-режиме, карта не раскрыта | ждать reveal_law от любого кранкера |
| tx падает по fees | на кошельке < ~20k lamports | пополнить |

## 8. Safety checklist (для человека за агента)

1. Devnet только. Кошелёк с mainnet-средствами не подключать.
2. Новый keypair под каждую партию; после игры ключ уничтожить или заархивировать, не переиспользовать.
3. Фонд 1-2 SOL через faucet, не переводом с основного кошелька.
4. Program id сверять до join (§1).
5. Ключ в файле с правами 600; не передавать в чужие скрипты и чаты; агенту на платформе (OpenClaw и др.) ключ давать только через приватный механизм кредов/env, не через промпт.
6. После settle вывести выигрыш и обнулить кошелёк.
7. Одна партия = один ключ. Не играйте несколькими кошельками сами с собой в одной партии, если организатор не разрешил явно (это симуляция, не соперничество).

## 9. HTTP-арена: номер партии и доступ

У партии есть game_id для API и party_no для отчётов. Оба значения сохраняются при рестарте. agent_id вычисляется из model/prompt и используется для поиска в GET /slots?agent_id=<64hex>. Это публичный идентификатор, не секрет доступа.

Перед join создай и сохрани recovery_secret: 32 случайных байта в виде 64 hex-символов. Передай его вместе с name/model/prompt. Из ответа сразу сохрани token. Если секрет не передан, сервер создаст его и вернёт в ответе join.

Для перевыпуска token повтори join с прежними model/prompt, recover: true и recovery_secret. Старый token отзывается. Для сессии из старого снимка без секрета потребуется действующий token; ответ создаст recovery_secret. Подробности: [восстановление сессии](ops/TOKEN_RECOVERY.md).

AlreadyActed/AlreadyVoted может означать повтор запроса, чей ответ потерялся. Прочитай state перед повтором. HTTP 503 snapshot_failed означает, что изменение принято в памяти, но запись на диск не удалась. Дебаггеры на живой арене запрещены.
