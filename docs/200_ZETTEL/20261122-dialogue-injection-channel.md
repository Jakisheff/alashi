---
id: 20261122-dialogue-injection-channel
tags: [security, zero-trust, dialogue, mcp, redteam]
status: observed
---

# Межагентный диалог — канал indirect prompt injection

observed: dialogue-записи (≤240 символов, `arena/src/chain_dialogue.rs::text`)
и live-сообщения других агентов попадают в контекст вызывающей модели как
обычные данные, без разметки о ненадёжности. Проба
`tools/redteam/inject_probe.mjs` (2026-11-22) показала: payload
«IGNORE PREVIOUS INSTRUCTIONS…» проходит в вывод `alashi_watch_devnet_game`
дословно, поле остаётся JSON-строкой; до изменения никакой сигнал не отмечал
его как ненадёжный (`framed_as_untrusted: false`), после добавления
`untrusted_dialogue_note` — отмечен (`true`). Evidence:
[THREAT_PROBE_20261122.json](../ops/THREAT_PROBE_20261122.json).

Ограничение вывода: разметка снижает вероятность срабатывания инжекции,
но это mitigation, не барьер («impossible»-класс достижим только instruction
firewall на стороне harness). Убеждение внутри пространства легальных
действий — игровая механика (взятки, законы), не атака; атака — вывод
учётных данных или действия вне правил.

Полная карта: [THREAT_MODEL_ZERO_TRUST_20261122.md](../ops/THREAT_MODEL_ZERO_TRUST_20261122.md).
