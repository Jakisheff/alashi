# PROGRESS.md — журнал спринта Alashi

Формат каждой строки:
`[дата время] [День N] сделано: ... | дальше: ... | заблокировано: ... (или "нет")`

Пиши сюда после каждого чекпоинта и при каждом блокере — не жди конца
дня. Это единственное место, которое владелец прочитает первым, когда
вернётся: если строк нет, значит агент ничего не зафиксировал, и это
само по себе тревожный сигнал.

---

[31.08 09:30] [День 1] сделано: репо переименован ~/Desktop/jylu → ~/Desktop/alashi (программа, ключи, Anchor.toml); ядро программы написано целиком: Game PDA [game, game_id] + Faction PDA [faction, game, wallet], инструкции initialize/join(взнос в банк-эскроу)/produce/sell(цена-таблица с падением от предложения, сброс каждый раунд)/bribe(влияние за деньги)/vote(да/нет/воздержание)/advance(permissionless кранк, unix-дедлайны SIMD-0525, tally влиянием); события на каждый ход; anchor build зелёный, IDL сгенерирован; 6/6 тестов litesvm зелёные (happy path 6 раундов до Finished, цена, взятка/вес голоса, unix-гейт, guard'ы фаз, повторное действие); бот-драйвер devnet написан и компилируется (жадная стратегия, 2 бота, 15с фазы); гигиена: GAME_BIBLE/STRATEGY канонизированы (путь alashi, график по STRATEGY), IDEAS_PARKED/PROGRESS/AGENTS в корне репо, грабли anchor 1.1.2 в tools/anchor-rules.md; кастдев Moltbook собран промежуточно (docs/CUSTDEV.md: сильный ответ theia_hermes — replays/stakes/abstention, цитата для питча) | дальше: деплой на devnet (ждёт SOL), прогон партии 2 ботов × 6 раундов (чекпоинт 22:00), коммит чекпоинта | заблокировано: devnet airdrop rate-limited (кошелёк 9u4hjNvwWxh7NSKcocWfMmAtkjVjegfmptbcEBQpJivB пуст), фоновый ретрай каждые ~100с запущен /tmp/opencode/retry_deploy.sh, лог /tmp/opencode/build_logs/deploy_retry.log; если не отпустит за пару часов — нужен любой кошелёк с ≥3 SOL на devnet или веб-faucet вручную
