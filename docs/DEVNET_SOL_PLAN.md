# DEVNET_SOL_PLAN — как закрыть потребность 3.2–3.5 SOL до демо (01.09 03:12)

Потребность по факту: рент программы 338 КБ ≈ 2.5 SOL + 0.5 (телефон жюри)
+ 3 × 0.05 (взносы) + комиссии ≈ 3.2-3.5 SOL. Кошелёк:
9u4hjNvwWxh7NSKcocWfMmAtkjVjegfmptbcEBQpJivB (faucet.solana.com
rate-limited с 30.08 утра, автоворонка RPC requestAirdrop безуспешна >30 ч).

## Пути по скорости

1. Перевод от человека (минуты): сообщение в чат Superteam KZ с адресом,
   3-4 devnet SOL. Fastest, ноль техники. Текст в TRIZ_ROUND2.A.
2. QuickNode faucet (сейчас, руками): faucet.quicknode.com/solana/devnet,
   ЖИВОЙ, Solana devnet поддерживается, 1 drip / 12 ч, нужен твит.
   drip сразу + второй к утру.
3. Localnet: fallback для экранкаста (партии записаны), минус — нет
   публичной ссылки в эксплорере для жюри.
4. Дешёвый mainnet (паттерн Sekaigent/0G): после аудита, не для демо.

## Не работает / не подтверждено

- solana airdrop CLI: НЕ отдельный лимит, это тот же официальный сервис
  (наш ensure_funds уже дёргает requestAirdrop).
- Alchemy faucet: недоступен из этой среды (transport error), статус
  неизвестен. Chainstack: 404. Helius: публичного крана не найдено.
- Широкоизвестность rate-limit 2026: не проверено в сессии.

## Конкуренты (по репо, не догадка)

Sekaigent: 0G Mainnet + NEXT_PUBLIC_USE_MOCKS для локалки. Daemon Hall:
собственный faucet-mint-worker ($5 промо) + TEE-цепь. Joule: Sepolia со
встроенным минтом USDC в приложение. Devot: 0G Galileo тестнет (кран не
назван) + mock-умы. Ask Trivium: offline mock с честной плашкой.
Mosaic: не ончейн. Вывод: никто не воюет с faucet.solana.com, наша боль
специфична для «Solana devnet + публичный эксплорер».
