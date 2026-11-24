#!/usr/bin/env python3
"""incident_taxonomy.py: таксономия операционных ошибок агента за 22-24.11
и rule-effectiveness аудит. Сырьё — PROGRESS-записи и git-история;
каждый инцидент: феномен / латентное условие / барьер (существовавший
на момент и почему не сработал). Классы:
  act-wider-than-intent — операция шире замысла оператора
  form-not-mechanism    — починена форма, не механизм
  blind-surface         — поверхность не покрыта ничьей проверкой
  journal-lie           — запись утверждает то, чего нет в коде
"""
import json, subprocess, statistics

REPO = "/Users/amir/Downloads/alashi"
log = subprocess.run(["git","-C",REPO,"log","--format=%ct","--since=2026-11-22"],
                     capture_output=True,text=True).stdout.split()
commits = len(log)

INCIDENTS = [
 {"id":"I1","ts":"22.11 22:31","class":"blind-surface",
  "phen":"attestation-поле ушло в POST арены спрэдом; 2 теста красные, Node-набор зелёный",
  "latent":"закрытая схема существует только на Rust-стороне; Node собирает тело спрэдом",
  "barrier":"отсутствовал; mock-контрагент не валидирует схему"},
 {"id":"I2","ts":"22.11 22:25","class":"blind-surface",
  "phen":"X-Alashi-Key-Env принимал имя служебной переменной (ALASHI_EGRESS_SEED → Bearer)",
  "latent":"фильтр проверял синтаксис имени, не множество допустимых",
  "barrier":"отсутствовал"},
 {"id":"I3","ts":"22.11 03:05","class":"journal-lie",
  "phen":"запись «K1 убит безусловным ассертом» — ассерта в коде не было 14 часов",
  "latent":"правка потеряна среди git checkout откатов seed-раундов ДО коммита",
  "barrier":"журнал append-only, не сверяется с кодом"},
 {"id":"I4","ts":"22.11 ~01:30","class":"act-wider-than-intent",
  "phen":"git checkout -- после киллер-прогона снёс ВСЕ незакоммиченные правки (idempotent-код исчез)",
  "latent":"оператор мыслит «откатить киллер», git выполняет «откатить файл»",
  "barrier":"отсутствовал; поймал check_all (не я)"},
 {"id":"I5","ts":"23.11 17:35","class":"act-wider-than-intent",
  "phen":"git checkout -- при втором киллере снова снёс незакоммиченные фиксы (повтор I4 тем же днём)",
  "latent":"то же; урок из I4 не стал процедурой",
  "barrier":"запись в PROGRESS (текст) — не сработала"},
 {"id":"I6","ts":"23.11 ~18:00","class":"form-not-mechanism",
  "phen":"«починил» латентностный тест: apply добавлен, но замороженные часы → apply не вызывался",
  "latent":"тест зелёный по другой причине; нет positive control",
  "barrier":"отсутствовал; поймал слепой контроль R4"},
 {"id":"I7","ts":"23.11 ~19:00","class":"form-not-mechanism",
  "phen":"redirect-киллер зелёный из-за DNS-фейла evil.example, не из-за redirect:'error'",
  "latent":"тест проходит по побочной причине",
  "barrier":"отсутствовал; пойман holdout-анализом"},
 {"id":"I8","ts":"23.11 23:11","class":"act-wider-than-intent",
  "phen":"rsync-песочницы с общим CARGO_TARGET_DIR распухли до 21.8 ГБ → ENOSPC",
  "latent":"общий target для разных исходных корней = новые отпечатки путей, гигабайты на прогон",
  "barrier":"df не мониторился перед прогонами"},
 {"id":"I9","ts":"24.11 12:22","class":"act-wider-than-intent",
  "phen":"merge-резолв: выбрана сторона ЛОКАЛЬНОГО main с приватными маркерами; grep после пуша",
  "latent":"сторона конфликта выбрана по предположению «HEAD чище» без проверки",
  "barrier":"правило п.20 СУЩЕСТВОВАЛО (grep перед пушем) — не исполнилось: текст, не ловушка"},
 {"id":"I10","ts":"23.11 00:52","class":"blind-surface",
  "phen":"первый фикс C2 (usage-аттестация) пропускал тело провайдера мимо квитанции",
  "latent":"подпись не проверялась, usage брался из тела",
  "barrier":"отсутствовал; пойман анонимным аудитором"},
 {"id":"I11","ts":"24.11 ~03:30","class":"blind-surface",
  "phen":"бюджет от закоммиченных квитанций слеп к сбою коммита (провайдер 5 раз, healthz 0)",
  "latent":"счётчики инкрементировались до коммита",
  "barrier":"отсутствовал; пойман слепым раундом"},
 {"id":"I12","ts":"23.11 02:59","class":"journal-lie",
  "phen":"placeholder-строка в do_reveal ломала reveal на любом tampered payload",
  "latent":"черновой код ушёл в коммит без прогона своей ветки",
  "barrier":"selfapply отсутствовал на тот момент"},
]

RULES = [
 {"rule":"R1 report-contract fixture","introduced":"23.11 00:28","covers":"blind-surface(contract)","exposure":3,"caught":3,"kind":"barrier","note":"двусторонняя заморозка схемы; M1/M2 убиты с обеих сторон"},
 {"rule":"R2 positive controls + kill attribution","introduced":"24.11 17:12","covers":"form-not-mechanism","exposure":4,"caught":4,"kind":"barrier","note":"baseline-green до инъекции; env-fail ≠ kill"},
 {"rule":"R3 fix-killer (мутант-убийца к каждому фиксу)","introduced":"23.11 17:35","covers":"form-not-mechanism","exposure":6,"caught":5,"kind":"barrier","note":"один выживший (K1) пойман battery — правило работало с задержкой"},
 {"rule":"R4 falsify selfcheck в check_all","introduced":"23.11 23:40","covers":"catalogue rot","exposure":5,"caught":5,"kind":"barrier","note":"мутационная стойкость в каноне"},
 {"rule":"R5 «хирургический откат киллера» (текст)","introduced":"24.11 02:05","covers":"act-wider-than-intent(checkout)","exposure":1,"caught":1,"kind":"symptom","note":"правило-симптом: сработало ПОСЛЕ второго инцидента того же класса; ловушки-кода нет"},
 {"rule":"R6 п.20 grep маркеров перед пушем (текст)","introduced":"18.11 (до сессии)","covers":"act-wider-than-intent(push)","exposure":9,"caught":8,"kind":"symptom","note":"существовал ДО I9 и не сработал в merge-резолве: текст, не ловушка"},
 {"rule":"R7 plan/факт + мини-отчёты (текст)","introduced":"01.09","covers":"process","exposure":"~40","caught":"~40","kind":"barrier","note":"поймал многократно (пропуски оценок) — но не ошибки выполнения"},
 {"rule":"R8 external_check.py (обязательный внешний пересчёт)","introduced":"24.11 23:40","covers":"journal-lie(reveal)","exposure":2,"caught":2,"kind":"barrier","note":"ловушка-код: reveal-лжец пойман моделью и selfapply"},
]

classes = {}
for i in INCIDENTS:
    classes[i["class"]] = classes.get(i["class"],0)+1
total_ops = commits + 12  # коммиты + merge + резолвы + киллеры
print(json.dumps({
 "operations_two_days": total_ops,
 "my_operational_incidents": len(INCIDENTS),
 "class_distribution": classes,
 "class_share": {k: round(v/len(INCIDENTS),2) for k,v in classes.items()},
 "act_wider_share_of_my_errors": round(classes.get("act-wider-than-intent",0)/len(INCIDENTS),2),
 "rules": [{"rule":r["rule"],"kind":r["kind"],"exposure":r["exposure"],"caught":r["caught"],
            "catch_rate": (round(r["caught"]/r["exposure"],2) if isinstance(r["exposure"],int) else "n/a"),
            "note": r["note"]} for r in RULES],
 "symptom_rules": [r["rule"] for r in RULES if r["kind"]=="symptom"],
 "barrier_rules": [r["rule"] for r in RULES if r["kind"]=="barrier"],
 "coverage_gaps": ["merge-резолв (до I9 ничто не проверяло стороны)",
                   "git push (grep — текст в AGENTS.md, не hook)",
                   "откат правок (checkout/ stash — вне harness)",
                   "дисковое пространство перед прогонами"],
}, ensure_ascii=False, indent=1))
