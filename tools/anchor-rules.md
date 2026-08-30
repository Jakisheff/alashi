# anchor-rules — правила сборки для агента (репо alashi)

## Окружение (intel-Mac)
- anchor-cli 1.1.2: ~/.local/bin/anchor
- solana-cli 4.2.1 / Agave: ~/.local/share/solana/install/active_release/bin — НЕ в PATH по умолчанию. Каждую сессию:
  export PATH="$HOME/.local/share/solana/install/active_release/bin:$HOME/.local/bin:$HOME/.cargo/bin:$PATH"
- rust 1.89.0 (rust-toolchain.toml), devnet-конфиг ~/.config/solana/cli/config.yml, кошелёк ~/.config/solana/id.json

## Грабли anchor 1.1.2 (проверено 31.08, все стоили времени)
1. programs/alashi/src/instructions/mod.rs обязан содержать `pub use <модуль>::*` для КАЖДОГО файла инструкций. Без этого #[program] падает с "unresolved import crate" (макрос ищет crate::__client_accounts_* в корне крейта, glob-реэкспорты их туда поднимают).
2. Vec<Account<T>> и Vec<AccountInfo> в Accounts-структуре НЕ работают (не реализован AccountsExit, кодоген __client_accounts_vec не резолвится). Динамический набор аккаунтов — Context::remaining_accounts + Account::<T>::try_from(&account_info).
3. CpiContext::new первым аргументом ждёт Pubkey, не AccountInfo: CpiContext::new(anchor_lang::system_program::ID, accounts).
4. Тесты — litesvm 0.10, без валидатора (skip_local_validator = true): cargo test -p alashi. Байты программы — include_bytes из target/deploy/alashi.so, поэтому сначала anchor build, потом тесты.
5. anchor build ~8 мин на intel-Mac. Запускать в фоне и не ждать staring на экран.

## Команды
- сборка: anchor build
- тесты: cargo test -p alashi
- деплой devnet: anchor deploy (provider и cluster уже в Anchor.toml)

## Соглашения кода
- таймеры фаз — unix (Clock::get), НЕ слоты (SIMD-0525); в тестах phase_duration = 0
- деньги: банк = Lamport'ы на PDA game; 1 песо = constants::PESO = 1_000_000 lamports; USDC-devnet — после 4.09
- конституционные константы (рейк 5%, делёж 50/30/15/5, банк, выплаты) — только constants.rs, законы их не меняют (фильтр Талеба, R22)
- аккаунты: Game [game, game_id], Faction [faction, game, wallet]; штампы acted_stamp/voted_stamp = (round << 3) | phase вместо сброса флагов
