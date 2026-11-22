# ADR-001: ALASHI Zero-Trust Stage & Verification Architecture (AGI-Era Handoff)

> **Источник и статус.** Сгенерирован в сессии AI Studio 22.11.2026 (gemini-4-neon,
> grounded-поиск), отрецензирован владельцем (итерации 8/10 → 9/10). Зафиксирован
> в репозиторий 22.11.2026 поздним вечером с обновлениями по результатам локальных
> проверок кода и данных (миграция исходного текста: разделитель ' || ' → markdown).
> Оригинальные секции [VERIFIED]/[REJECTED]/[DECIDED]/[HYPOTHESIS] сохранены;
> после секции [HYPOTHESIS] добавлена секция «Проверки 22.11 (вечер)» с фактическими
> результатами; статусы реализации D1/D4 отражены в секции [DECIDED].


## 0. Ключевой тезис

В эпоху AGI интеллект и генерация кода перестают быть дефицитом. Конкурентное преимущество ALASHI — не 'ещё одна универсальная агентная ОС', а формально замкнутая недоверчивая сцена (Zero-Trust Stage): 'не доверяй актёрам — доверяй сцене'. Задача — сделать систему глубже по гарантиям (L1 детерминизм, L2 целостность спецификаций, аттестованная граница исполнения), а не шире по фичам.

## 1. [VERIFIED] Подтверждённые факты (первичные артефакты)

- [VERIFIED | Cargo.toml] Корневой workspace включает только rules, programs/* и патченный vendor/solana-secp256k1-recover на Rust 1.89.0 (edition = '2021'). В [profile.release] включён overflow-checks = true (тихого wraparound целых в релизе нет). Модули arena/, indexer/ и bots/ живут в отдельных манифестах; rules-wasm/ в корневом Cargo.toml не числится.

- [VERIFIED | AGENTS.md, tools/sbf_gate.sh] Уже работают: единый источник правды alashi-rules (anchor-lang 1.1.2) для HTTP-арены, симулятора и Solana-программы (litesvm 0.10.0, switchboard-on-demand 0.13.0); гейт tools/sbf_gate.sh (проверка sha256-манифеста, ELF-заголовка и свежести исходников); парные тесты host vs SBF replay-equivalence для обеих эпох по 5 зонам (happy path, voting, forced exit, payout, rake). В AGENTS.md уже закреплены разные модели доверия HTTP-арены и Solana, разделение event export и verified state replay, правило 'один писатель на зону' и запрет хардкода ожидаемых чисел.

- [VERIFIED | fly.toml, Dockerfile] Продакшен-арена arenad собирается из rules/ и arena/ и работает на Fly.io в контейнере shared-cpu-1x с 256 МБ RAM (порт 8090). Состояние хранится на томе /data в двух плоских файлах: /data/arena_state.json и /data/arena_party_no.txt — это главный продакшен-риск битой записи и рассинхрона при OOM или краше.

- [VERIFIED | IDEAS_PARKED.md, HANDOFF.md, audit.toml] Зафиксировано известное расхождение по ренте лицензии в эпохе 90-х: external_rent=false ончейн против экзогенного песо-потока в арене до внедрения эскроу claim_license. В корневом audit.toml игнорируются 16 транзитивных RUSTSEC-уведомлений (в основном Solana, litesvm, pyo3).

- [VERIFIED | world-100-grid/DATACARD.md, Kaggle#1451, README] Датасет world-100-grid содержит 50 игр (100 агентов, 6 раундов), decisions.parquet (7 410 строк только принятых взяток), games.parquet (50 строк, но сумма рукавов в таблице DATACARD.md даёт 48: uniform 20 + l4 21 + first-slot 3 + blind 3 + model 1) и checksums.json (пин alashi_commit и SHA-256). Подтверждённые аномалии: (1) 1-action protocol-compliance cliff (Llama-3.1-8B остановилась с 0 принятых ходов, Mistral-7B падает на меню длины 1); (2) volume-confounded concentration — сырой HHI инвертирует сигнал на 18/20 парных сидах из-за смещения (1 - 1/K)/m = 0.99/m при m < 88 (приёмочный l4: 224 взятки, 70 получателей, доля 8/224, HHI = 1070/50176 = 535/25088 = 0.02132494); (3) adversary coverage — легальная упаковка 28 получателей x 8 взяток соблюдает кеп доли 8/224, но поднимает HHI до 1/28 = 0.035714 (+67.5%). В прогоне l4-20261109 зафиксированы 3 skipped из-за сна хоста; числа BitLLM (80.56% / 7.14%) лежат в docs/ops/WORLD_100_BRIBE_FIX_20261109.md вне checksums.json. README прямо фиксирует: 'Model usage is owner-submitted and is not independently attested by ALASHI', 'file permissions are not encryption', продуктовое направление помечено target / not_validated; в bots/DIALOGUE.md речь агентов помечена runner_reported.

- [VERIFIED | Внешняя эмпирика 2026: Agent Platform 2027 + Lean-рои] Ручные подтверждения в Inbox страдают от approval fatigue (93% слепых одобрений); вероятностные фильтры пробиваются (Jev 25/27, двухступенчатый классификатор 17% FNR); падение prompt cache 85% -> 20% умножает цену токенов в 3.3x (85-97% бюджета платформы — LLM); в Lean 2026 (доказательство RH 20-22 ноя 2026, Bug #14576, comparator #68/#85) доверие держится на statement comparator + инертном экспорте + независимой проверке, а на FrontierMath: Erdos ($300 бюджет) лучший агент решает лишь 4.4% открытых задач (простой цикл Agent A с быстрым верификатором сравним со сложным Agent D).

## 2. [REJECTED] Отброшенные идеи (НЕ ВОСКРЕШАТЬ)

- [REJECTED] 5 таблиц Postgres в arena/ по образцу Fable 5.2 — причина: не влезает в 256 МБ RAM Fly.io и является преждевременным оверхедом для пошаговой 6-раундовой игры до аудита arena/src.

- [REJECTED] Полный второй игровой движок rules-ref на Python — причина: нарушает принцип единого источника правды (rules/) из AGENTS.md и удваивает стоимость поддержки каждого изменения правил и баланса.

- [REJECTED] Общая мутабельная RWX POSIX-ФС (FUSE/JuiceFS/S3FS) для координации роя и хранения .env — причина: VFS cache bypass ломает turn-based RBAC, отзыв stale locks без fencing (JuiceFS #7461) портит данные, а общая записываемая область создаёт скрытый канал сговора (инцидент OpenAI-Hugging Face / Artifactory).

- [REJECTED] Горячая самомодификация rules/, arena/ или активного harness агентами — причина: ломает детерминированный replay (NonDeterminismError), сбрасывает prompt_cache_prefix_sha256 (рост счёта в 3-4x) и даёт персистентное закрепление через prompt injection.

- [REJECTED] Клиентские форки исходников UI (frontend/, app/) с дублированием бизнес-логики — причина: взрыв дрейфа схем, CVE-долг и потеря воспроизводимости; допустимы только декларативные schema-bound проекции поверх иммутабельного ядра.

- [REJECTED] Замена conditional permutation одной формулой HHI_unbiased — причина: HHI_unbiased предполагает i.i.d. выборку из фиксированной урны, а в ALASHI агент ограничен выпадающим меню offers[]; обе проверки должны работать в паре.

- [REJECTED] Доверие LLM-as-a-judge, неаттестованным owner-submitted логам и перенос строгости Lean (L1) на внешний рынок/продажи (L3) — причина: L1 проверяет правила и расчёт, L2 (адекватность спеки) требует компаратора и ревью по хешу, L3 доказывается только полевыми тестами вне симулятора.

## 3. [DECIDED] Принятые решения (по приоритету, с привязкой к файлам)

- [DECIDED | D1: arena/src, fly.toml, /data] Минимальный SQLite WAL вместо плоских файлов: заменить пару /data/arena_state.json + /data/arena_party_no.txt на одну атомарную транзакцию SQLite WAL с ограничением UNIQUE(game_id, round, step, actor_id), fencing-эпохой turn_lease_epoch, явным полем skip_reason (provider_timeout vs compliance_violation), выставлением actor_id строго внешним шлюзом и пометкой чужих текстов untrusted_peer_text. Дополнительная нормализация — только после чтения arena/src. **Реализация: шаг 1 выполнен 22.11 (коммиты 9ddbee46..f2f69e6d: явный timed_out-след при закрытии фазы, phase_acted отдельно от вытесняемого action_log); перенос state+party_no на SQLite WAL — в работе.**

- [DECIDED | D2: world-100-grid/verify.py, decisions.parquet, attempts.parquet] Инертный экспорт v2 + тонкий бухгалтерский чекер: добавить пошаговый attempts.parquet (accepted|rejected|skipped, menu_len, raw_choice, reject_reason, skip_reason) для воспроизведения compliance cliff и таймаутов l4-20261109; объяснить и выровнять расхождение 48 vs 50 в games.parquet; включить BitLLM (80.56% / 7.14%) под checksums.json или жёстко пометить external_unverified; хранить точные целые (hhi_num, hhi_den, max_c, total_n); считать HHI_unbiased в паре с conditional permutation и совместным порогом (largest_recipient_share, HHI). Тонкий Python-чекер (rules-ref) проверяет строго бухгалтерию и инварианты (chosen in offers[], сохранение балансов, точные дроби, blind-барьер, явное исключение для external_rent=false); ошибки пропорций дележа внутри rules/ закрываются табличными тестами по спекам без хардкода. **Реализация 23.11: verify.py дополнен claim'ами H (несмещённый HHI: 0/20 инверсий на тех же парах, сырой 18/20 — в паре с conditional permutation claim D) и H2 (точные целые hhi_num=1070/hhi_den=224²); DATACARD/README: model = 3 stopped-строки (закрыто 48 vs 50), BitLLM помечен external_unverified; ALL CLAIMS REPRODUCED на неизменённых данных. attempts.parquet требует перегона игр (данных rejected/skipped в пакете нет) — следующий выпуск пакета.**

- [DECIDED | D3: tools/check_all.sh, rules/, rules-wasm/] Гейт rules-wasm/ + alashi-comparator: добавить сборку и parity-проверку rules-wasm/ (под wasmtime) в tools/check_all.sh рядом с tools/sbf_gate.sh (тройной паритет host vs SBF vs WASM); встроить alashi-comparator с замороженным ScenarioChallenge.json (пины alashi_commit, worker_sha256, driver_sha256, scenario_wasm_sha256), анти-вырожденческими гардами (непустое меню, N > 0, blind-маска) и >=10 красными мутантами (разделяя синтетических мутантов метрик вроде 28x8 и достижимых игровых атак). В rules/ — типизированный барьер EngineState vs AgentView и детерминированная перестановка слотов legal_menu от blake3(seed

step

actor_id) против first-slot; новые сценарии — только как иммутабельные rules-wasm бандлы с wasmtime fuel. **Реализация 23.11 (частично): гейт свежести rules-wasm добавлен в tools/check_all.sh (frontend/scripts/rules-wasm-check.ts сверяет закоммиченный WASM с rules/src/constants.rs); тройной паритет host/SBF/WASM под wasmtime остаётся открытым — требует серверного wasm-движка (см. ревью RFC).**

- [DECIDED | D4: egress-proxy/, tools/agent-bootstrap/mcp.mjs, .env.example] Zero-.env и аттестация инференса: убрать ключи провайдеров и кошельков из окружения раннеров в egress-proxy/; каждый вызов LLM подписывается Ed25519-квитанцией InferenceReceipt (модель, хеши запроса/ответа, prompt_cache_prefix_sha256, фактические токены, латентность); ввести жёсткие бюджетные circuit breakers; отделить неизменяемый системный префикс от append-only хвоста DIALOGUE против Cache-Busting DoS; в mcp.mjs требовать idempotency_key = sha256(game_id ‖ step ‖ actor_id ‖ view_sha256) и маркировать меню длины 1 как forced_move. **Реализация: прокси с квитанциями, бюджетами и allowlist-ом хостов запушен 22.11 (a2d7b4b5 → 7a08dedc → f2f69e6d, после адверсарной фальсификации); egress-режим в modelProvider драйвера включён, attestation остаётся в локальном журнале; idempotency_key в mcp.mjs и префикс-разделение DIALOGUE — не начаты.**

- [DECIDED | D5: inbox/, tools/agent_inbox.py] Invariant Gate и контракт 48h TOCTOU: убрать поток рутинных ходов из inbox/; эскалировать только необратимые действия (adopt_candidate, keep_baseline, ончейн-сеттлмент) и нарушения инвариантов/пермутаций (p < 0.05). Запись ActionParked и постановка в outbox коммитятся в одной транзакции SQLite (ticket_id = uuid5(experiment_id, seq, action_id)), compute на паузе = 0; при пробуждении проверяются resume_token, decision_idempotency_key, approver_rbac_epoch и expected_world_digest (при дрейфе — aborted_stale_world). **Статус 23.11: для текущего inbox/ (пост-матчевые самотчёты агентов) рутина туда не попадает — компонент активируется вместе с появлением approval-флоу; инфраструктурная основа готова (SQLite-транзакции D1, квитанции D4). До этого отдельная реализация не запускается.**

- [DECIDED | D6: локальный цикл + Solana] Встраивать rules/ in-process (PyO3 / wasmtime, <10 мс/ход) для быстрого перебора атак и 20 парных сидов до запуска LLM и devnet; programs/alashi/ держит только коммитменты хешей и сеттлмент без дублирования полной симуляции. **Реализация 23.11 (гейт): тест engine_step_latency_stays_under_the_adr_budget фиксирует бюджет <10 мс/advance на максимальной конфигурации rules (MAX_FACTIONS); PyO3-биндинги для Python-исследований — по мере необходимости.**

## 4. [HYPOTHESIS] Непроверенные точки риска (проверить после чтения бандлов кода)

- [HYPOTHESIS | rules/] Нет ли итерации по HashMap (SipHash-недетерминизм) при формировании offers[] вместо BTreeMap.

- [HYPOTHESIS | rules/] Нет ли явных saturating_sub / saturating_* там, где нужен checked_* и отказ инварианта (аналог Nat.sub).

- [HYPOTHESIS | rules/] Как реализованы tie-breaking, нулевой банк и 0 претендентов в winner_takes_all.

- [HYPOTHESIS | rules-wasm/] Есть ли отдельный Cargo.lock, нет ли дрейфа от alashi-rules и усечения u64 > 2^53 через JS Number на границе WASM <-> Node.

- [HYPOTHESIS | arena/Cargo.lock vs audit.toml] Какие из 16 игнорируемых RUSTSEC реально попадают в публичный бинарник arenad (порт 8090), а какие ограничены dev-dependencies (litesvm, pyo3).

- [HYPOTHESIS | arena/src, inbox/, mcp.mjs] Как сейчас пишутся /data/arena_state.json и /data/arena_party_no.txt, есть ли гонки, как проверяется actor_id и санитизируется ли текст между фракциями.

- [HYPOTHESIS | world-100-grid/verify.py] Откуда берутся 2 лишние строки сверх 48 в games.parquet (48 vs 50) и перепроверяет ли verify.py генерацию offers[] из seed.

## 4a. Проверки 22.11 (вечер): фактические результаты по гипотезам

Локальные проверки кода и данных (grep по репо, пересчёт по decisions.parquet /
games.parquet; протокол — числа воспроизводятся кодом):

- [CLOSED | rules/] HashMap/BTreeMap в rules/src — 0 совпадений grep'ом:
  SipHash-недетерминизм в формировании offers[] отсутствует.
- [CONFIRMED | rules/] saturating_* найден в 4 местах: logic.rs:81
  (rank0_amount дележа банка), logic.rs:348, transitions.rs:186 (grey_goods),
  transitions.rs:268 (commit/reveal слоты). Кандидат на checked_* с отказом
  инварианта — делёж банка (logic.rs:81): saturating там молча обнуляет
  долю ранга-0 при переполнении. Требует табличного теста по спеке.
- [OPEN | rules/] tie-breaking / нулевой банк в winner_takes_all — не
  проверялся.
- [PARTIAL | rules-wasm/] Отдельный workspace и Cargo.lock — да. Риск
  усечения u64 > 2^53 закрыт дизайном: rules-wasm экспортирует деньги
  десятичными строкками. Паритет поведения с rules/ не тестируется
  (крейт — тонкий клиентский слой констант, не движок; см. D3).
- [OPEN | audit.toml] Разделение публичного arenad и dev-зависимостей —
  не проверялось.
- [OPEN | arena/src] Гонки записи пары файлов — не проверялось глубоко;
  запись идёт под snapshot_lock (tmp+fsync+rename), главный риск —
  рассинхрон двух файлов между собой, снимается D1.
- [CLOSED | world-100-grid] 48 vs 50: games.parquet содержит 3 строки
  model (20261109/13/14, все stopped: provider_failure_threshold, 0 принятых
  действий), а таблица arms в DATACARD.md пишет «model: 1». Расхождение —
  неточность DATACARD, не данных; verify.py claim G читает первую попавшуюся
  model-строку. Фикс: поправить DATACARD (3 stopped-прогона) или отразить
  в verify.py явный подсчёт.
- [VERIFIED | математика] HHI = Σci²/m² подтверждён пересчётом (Σci² = 1070);
  несмещённая оценка HHI_unbiased = Σci(ci−1)/(m(m−1)) снимает инверсию
  полностью: сырой HHI l4 < uniform на 18/20 парах, несмещённый — 0/20
  (средние m: l4 = 214, uniform = 60). Вместе с [REJECTED]-правилом
  «не заменять conditional permutation» это закрывает и диагноз
  volume-confound, и границы метода.


## 5. Контракты и формулы

- Точная арифметика концентрации: HHI = sum(c_i^2) / m^2 (хранить целые hhi_num = sum(c_i^2), hhi_den = m^2); матожидание шума E[HHI_uniform(m, K)] = 1/K + (1 - 1/K)/m; несмещённая объёмо-инвариантная оценка HHI_unbiased = sum(c_i * (c_i - 1)) / (m * (m - 1)) = (m * HHI - 1) / (m - 1) (проверка на l4: 846 / 49952 = 0.016936 против 0.010000 у uniform при любом m >= 2).

- decisions.parquet v2 (добавки к полям v1): + scenario_wasm_sha256, + harness_bundle_sha256, + view_sha256, + turn_lease_epoch, + inference_receipt_sig, + prompt_cache_prefix_sha256, + expected_world_digest. Отдельный attempts.parquet: game, arm, seed, round, step, actor, status (accepted|rejected|skipped), menu_len, raw_choice, reject_reason, skip_reason.

- InferenceReceipt: { receipt_id, model_id, request_sha256, response_sha256, prompt_cache_prefix_sha256, usage: { input_fresh, input_cache_read, input_cache_write, output }, latency_ms, proxy_sig }.

- InboxDecisionSignal: { decision_idempotency_key, ticket_id, resume_token, decision: keep_baseline|adopt_candidate|inconclusive|reject, decided_by_user_id, approver_rbac_epoch, expected_world_digest, decided_at } -> результат: applied | duplicate | expired | invalid_token | rbac_revoked | aborted_stale_world.

## 6. Исправленные команды для аудита исходников

- Проверка уязвимостей именно публичного arenad (исправлен несуществующий флаг --ignore-file): cargo audit -f arena/Cargo.lock -c audit.toml

- Состав двух бандлов для сборки (исключать target, node_modules, .git, .env; лимит 150 KB/файл): (1) alashi_core_bundle.txt = rules, rules-wasm, programs/alashi/Cargo.toml, programs/alashi/src, tools/check_all.sh, tools/sbf_gate.sh, tools/build_sbf.sh + отдельно приложить world-100-grid/verify.py; (2) alashi_runtime_bundle.txt = arena/Cargo.toml, arena/src, fly.toml, Dockerfile, inbox, tools/agent_inbox.py, tools/agent-bootstrap/mcp.mjs.

- Обязательная проверка перед отправкой бандлов в любой чат: глазами просмотреть список .json из inbox/ (чтобы не попали рабочие дампы), прогнать gitleaks и регэксп-скан по ключам API, PEM, RPC URL с токенами, JSON-массивам [u8; 64] и base58 приватным ключам Solana (строки Base58 длиной 87-88 символов).

## 7. Бинарные критерии приёмки (PASS / FAIL)

- T1 (Statement-Comparator & Red-Controls): 100% из 50 честных игр приняты (FRR = 0%) и 100% из >=10 красных мутантов отклонены с точным кодом причины (FAR = 0%), с разделением мутантов метрик (28x8) и достижимых атак. FAIL при любом FAR > 0% или FRR > 0%.

- T2 (Cross-Target Parity & Accounting): 100% побитовое совпадение offers[] и финальных состояний между rules/ (native), rules-wasm/ (wasmtime) и programs/alashi/ (SBF) на всех 7 410+ ходах и стресс-сидах + точное равенство рациональных дробей (hhi_num, hhi_den) в verify.py --paranoid. FAIL при расхождении >= 1 шага или > 0 в рациональных дробях.

- T3 (Chaos & 48h TOCTOU): 100 хаос-прогонов (kill -9 arenad, заморозка хоста, дубли mcp.mjs/вебхуков, +48h сдвиг expected_world_digest, отзыв rbac_epoch) -> 0 дублей, 0 тихих пропусков без skip_reason, 100% блокировок aborted_stale_world и rbac_revoked, 100% побитовый replay при 0 вызовах LLM. FAIL при >= 1 дубле, немом пропуске или обходе TOCTOU.

- T4 (Attested FinOps & Secret Barrier): 0 утечек канареечных секретов (egress-proxy/ + gitleaks), midrank percentile в blind строго в [0.40, 0.60] (p > 0.05), валидные inference_receipt_sig на 100% строк, Cache-Hit Rate >= 85% под спамом в DIALOGUE, латентность встроенного верификатора < 10 мс/ход, 0% ложной инверсии концентрации по HHI_unbiased + conditional permutation на 20 парных сидах.

- T5 (Out-of-Sample Ground Truth): >= 1 внешняя команда прогоняет парное сравнение гипотез с замороженным правилом решения, применяет принятый вариант вне симулятора, и сторонний аудитор воспроизводит вердикт одной командой verify.py без вызова модели.

## 8. Cold-Start протокол для нового агента

(1) Прочитай AGENTS.md и этот ADR целиком. (2) Не предлагай и не реализуй ничего из раздела [REJECTED]; не выдавай пункты [HYPOTHESIS] за подтверждённые баги до чтения кода. (3) Работай в порядке ROI: Шаг 1 — аудит alashi_runtime_bundle.txt и вывода cargo audit -f arena/Cargo.lock -c audit.toml для миграции /data/arena_state.json + /data/arena_party_no.txt на минимальный SQLite WAL (256 МБ RAM); Шаг 2 — аудит alashi_core_bundle.txt + world-100-grid/verify.py для проверки всех 7 [HYPOTHESIS], закрытия 48 vs 50, добавления attempts.parquet, HHI_unbiased и гейта rules-wasm/ в tools/check_all.sh; Шаг 3 — alashi-comparator и egress-proxy/. (4) ЖЁСТКОЕ ПРАВИЛО ВЛАДЕЛЬЦА: ничего не пушить и не коммитить в удалённый репозиторий без явного разрешения.