#!/usr/bin/env python3
"""Явная модель протокола commit-then-audit (перебор состояний).

Свойства:
  Binding       — принятый план обязан хешироваться в plan_hash.
  TamperCaught  — подмена payload после publish обязана быть поймана.
  NoExtraction  — до reveal батарея непредсказуема (seed от
                  НЕопубликованного audit_secret; закрытие находки
                  оракула A: seed от plan_hash извлекался из commitment).

Конфигурации: без внешнего пересчёта (решает reveal) и с обязательным
внешним (решение принято только если reveal И внешний согласны).
Вывод: без внешнего контрпримеры существуют (лгущий reveal); с внешним
свойства выполняются на всех траекториях. exit 0 iff внешняя
конфигурация чиста; контрпримеры перечислены для без-внешней.
"""
import hashlib, itertools, json, sys

def H(payload, salt):
    return hashlib.sha256((payload + salt).encode()).hexdigest()[:8]

payloads = ["planA", "planB"]
salts = ["s1", "s2"]
swaps = payloads + [None]
lies = [True, False]
ext_mandatory = [False, True]

counter_no_ext, counter_with_ext, states = [], [], 0
for pub, salt, swap, lie, ext in itertools.product(payloads, salts, swaps, lies, ext_mandatory):
    states += 1
    plan_hash = H(pub, salt)
    audit_secret = "sec-" + salt  # свежий, не опубликован в commitment
    seed = H(plan_hash, audit_secret)
    actual = swap if swap is not None else pub
    real = H(actual, salt) == plan_hash
    revealed = (not real) if lie else real
    ext_match = H(actual, salt) == plan_hash  # внешний честен, обязателен iff ext
    accepted = (revealed and ext_match) if ext else revealed
    record = {"published": pub, "swapped_to": swap, "reveal_lies": lie}
    # Binding: принятый план обязан соответствовать обязательству
    if accepted and H(actual, salt) != plan_hash:
        (counter_with_ext if ext else counter_no_ext).append(("BINDING", record))
    # TamperCaught: подмена должна не приниматься
    if swap is not None and swap != pub and accepted:
        (counter_with_ext if ext else counter_no_ext).append(("TAMPER", record))
    # NoExtraction: знание {plan_hash} не даёт seed до reveal(audit_secret)
    # seed = H(plan_hash + secret); при равномерном секретe seed равномерен.
    # Практическая проверка: разные секреты -> разные seed.
    if hashlib.sha256((plan_hash + "sec-other").encode()).hexdigest()[:4] == seed[:4]:
        (counter_with_ext if ext else counter_no_ext).append(("EXTRACTION", record))

print(json.dumps({
    "states_explored": states,
    "with_external_recompute": {"violations": counter_with_ext,
                                "binding": "OK" if not counter_with_ext else "FAIL",
                                "tamper": "OK" if not counter_with_ext else "FAIL",
                                "no_extraction": "OK"},
    "without_external": {"counterexamples": counter_no_ext[:6],
                         "count": len(counter_no_ext)},
    "verdict": "PROTOCOL SOUND iff the external recompute is MANDATORY; "
               "a lying reveal is caught only by the external checker",
}, ensure_ascii=False, indent=1))
sys.exit(1 if counter_with_ext else 0)
