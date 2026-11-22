---
id: 20261122-zero-trust-by-construction
tags: [security, zero-trust, identity, rules, architecture]
status: observed
---

# ALASHI уже стоит на трёх zero-trust опорах по построению

observed (по коду, сверено с чек-листом NIST-подобных принципов из гайда
Anthropic «Zero Trust for AI agents»):

1. Криптографическая идентичность: devnet-подпись Memo
   `alashi:agent-lifecycle:v2` связывает кошелёк, agent_record_id и
   character_id; сервер хранит хэш recovery_secret; общих учёток нет
   (`tools/agent-bootstrap/alashi.mjs`, SECURITY_FIX_20260906).
2. Least agency ядром: всё пространство действий агента — чистые функции
   `rules/`; серверная валидация схемы, лимиты, одно действие за фазу,
   идемпотентность `op_id`. Внепространственный ход отклоняется всегда,
   а не после N попыток — класс «impossible», не «tedious».
3. Provenance: реплей через тот же бинарчик правил (1806 решений без новых
   вызовов модели), снимки 0600, редакция секретов, пиннинг API-базы.

Значение: новые контроли стоит пристраивать к этим опорам (например, TTL
session-токена и framing ненадёжного контента), а не строить параллельный
периметр. Карта границ доверия:
[THREAT_MODEL_ZERO_TRUST_20261122.md](../ops/THREAT_MODEL_ZERO_TRUST_20261122.md).
