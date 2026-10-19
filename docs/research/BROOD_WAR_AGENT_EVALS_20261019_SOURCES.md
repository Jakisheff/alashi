# Источники: Brood War и оценка агентов

Проверено 19 октября 2026. Основной [отчёт](BROOD_WAR_AGENT_EVALS_20261019.md) отделяет наблюдения по ALASHI от описаний внешних авторов. [Playbook](BROOD_WAR_PLAYBOOK.md) составлен заново по открытым разделам Blizzard. Полные чужие тексты и пользовательские оригиналы в репозиторий не включены.

## Как проведена проверка

Сначала выполнен поиск по локальным материалам владельца, включая Downloads, каноническую папку исследований и прежние вложения Codex. Найдены два оригинала о дирижёре; текст извлечён из DOCX без пересказа и прочитан локально. Затем сопоставлены текущие правила ALASHI и вызывающий их слой strategy cycle.

В интернете проверялись материалы авторов: документация Blizzard, исходная расшифровка интервью, публикация создателя Ralv, статьи исследователей и документация разработчиков. Поисковые запросы сочетали `StarCraft agents January 2026`, `agent orchestration benchmark`, `agent evaluation cost reliability` и названия найденных работ. Вторичные пересказы использовались как указатели; технические выводы опираются на первичные источники ниже. Это целевая исследовательская выборка, а не систематический обзор всей литературы или подсчёт публикаций X.

Наличие страницы проверено открытием. Это не архивирование её содержания: динамические рейтинги могут измениться. Цифры текущих лидеров не переписывались. Результаты внешних исследований самостоятельно не воспроизводились. Для препринтов «опубликовано» означает размещение рукописи, без утверждения о рецензировании.

## Правила и практика Brood War

| Источник | Что прочитано и используется | Ограничение |
| --- | --- | --- |
| [Blizzard Strategy Central](https://classic.battle.net/scc/gs/) | Индекс оригинального Compendium | Учебный архив, не обзор меты 2026 |
| [Resources](https://classic.battle.net/scc/gs/res.shtml) | Экономика добычи с учётом насыщения | Число рабочих из старого руководства не используется как универсальная норма |
| [Reconnaissance](https://classic.battle.net/scc/GS/recon.shtml) | Цена разведки, обновление сведений, проверка итоговой статистики | Советы не дают гарантированной победы |
| [Hot Keys and Special Commands](https://classic.battle.net/scc/gs/control.shtml) | Группы и очереди приказов | Компьютерная StarCraft/Brood War, не SC2 |
| [Protoss Strategy](https://classic.battle.net/scc/protoss/astrat.shtml) | Производство во время боя, дистанция, выбор состава | Исторические примеры не объявлены текущими открытиями |
| [High Ground and Cover](https://classic.battle.net/scc/gs/hc.shtml) | Значение местности | Точные вероятности попадания в сравнении не используются |
| [Observer](https://classic.battle.net/scc/protoss/units/observer.shtml) и [Observatory](https://classic.battle.net/scc/protoss/popup/observatory.htm) | Пример технологической предпосылки для обнаружения | Это одна ветка, не полный build order |
| [SSCAIT rules](https://sscaitournament.com/index.php?action=rules) | Наблюдения, паузы, вычисления, replay, происхождение ботов | Регламент конкретной лиги; её текущая активность отдельно не проверена |
| [SMACv2 README](https://github.com/oxwhirl/smacv2) | Разделение микроконтроля и полной игры | StarCraft II и cooperative MARL, не оценка человека с LLM |

## Метафоры и январское сравнение

| Источник и дата | Что прочитано и подтверждено | Ограничение |
| --- | --- | --- |
| [David Senra: Tobi Lütke](https://www.davidsenra.com/episode/tobi-lutke), 18.01.2026 | Дата и исходная расшифровка, фрагмент StarCraft/Factorio/шесть агентов | Подтверждает высказывание; первенство автора и конкретный твит Амира не установлены |
| [Dominik Scholz: The provenance crisis](https://dipl.ing/blog/the-provenance-crisis), 30.09.2026 | Авторская ретроспектива Ralv с январскими постами и сентябрьским офисом агентов | Встроенные посты доступны через публикацию автора; их охват независимо не измерялся |
| [AgentOrchestra v6](https://arxiv.org/abs/2506.12508v6), первая версия 14.06.2025, v6 28.05.2026 | Abstract и история версий; TEA и иерархический planner | Описание авторов, не независимый сравнительный аудит |
| [Anthropic Managed Agents](https://www.anthropic.com/engineering/managed-agents), 08.04.2026 | Интерфейсы session/harness/sandbox и восстановление; аналогия с ОС | Инженерный опыт одного разработчика платформы |
| [Deutsche Grammophon: процесс Orpheus](https://www.deutschegrammophon.com/en/artists/orpheuschamberorchestra/news/orpheus-chamber-orchestra-present-their-complete-recordings-on-deutsche-grammophon-263131), 2021 | Коллективное руководство с распределением и сменой ролей | Контрпример к обязательной централизации; не эксперимент над ИИ |

## Оценивание

| Источник и версия | Что прочитано | Граница вывода |
| --- | --- | --- |
| [Anthropic Demystifying evals](https://www.anthropic.com/engineering/demystifying-evals-for-ai-agents), 09.01.2026 | Термины проверки и повторы; различие trace/outcome | Практическое руководство; не единая шкала всех агентов |
| [ClawArena-Team v1](https://arxiv.org/html/2606.31174v1), 30.06.2026 | Методы и scoring; ограничения и pricing appendix | Фиксированный пул ограничивает переносимость; код и данные обещаны. Локальные работники не включены в API-price управляющего |
| [OrchBench v1](https://arxiv.org/html/2607.25656v1), 28.07.2026 | Постановка, симулятор, пропущенные handoff, проверки сгенерированных DAG | Структура задачи задана; модель качества имеет допущения. Симуляция не заменяет реальное исполнение |
| [Scaling Agent Systems v3](https://arxiv.org/abs/2512.08296v3), 08.04.2026 | Abstract и версии: 260 конфигураций в v3 | У ранних версий иное число конфигураций; проценты улучшений не объявлены универсальным законом |
| [Terminal-Bench 4.0](https://www.tbench.ai/news/terminal-bench-4-0), 28.08.2026; [Continuous Benchmarks](https://www.tbench.ai/news/continuous-benchmarks), 30.07.2026 | Калибровка ресурсов и исправления; семантические версии | Сравнение результатов разных major-версий требует повторного испытания |
| [APEX-Agents 1.1](https://www.mercor.com/blog/introducing-apex-agents-1-1/), 08.09.2026 | Рабочие задания и scattergunning; аудит и калибровка judge | Judge ошибается; творческое качество рекламы и её конверсия остаются другими проверками |
| [τ-bench repository](https://github.com/sierra-research/tau2-bench), текущий README | τ³-bench, knowledge/voice, изменение grading в июле 2026 | URL содержит старое имя tau2; результаты до и после исправления banking grading несопоставимы напрямую |
| [Vending-Bench 2](https://andonlabs.com/evals/vending-bench-2) | Среда, критерий, system prompt и API-cost plot | Симуляционный бизнес, внутриигровой штраф и API-стоимость различаются |
| [Vending-Bench Arena](https://andonlabs.com/evals/vending-bench-arena) | Методы и правила конкурентных запусков; датированные раунды 2026 | Индивидуальный экономический итог; человеческое управление отдельно не измеряется |
| [Kaggle Game Arena v1](https://arxiv.org/html/2609.31473v1), 25.09.2026 | Устройство эксперимента и сред; метрики с интервалами | Выбор моделей зафиксирован на февраль 2026; место в игре не универсальный рейтинг |
| [METR Time Horizons](https://metr.org/time-horizons/), обновление 08.09.2026 | Определение и домены; ограничения и прекращение активного обновления | Время человека на задачу отличается от времени запуска агента |
| [METR Expenditure Horizon](https://metr.org/blog/2026-07-21-expenditure-horizon/), 21.07.2026 | Метод, NanoGPT-пример, повторная проверка и ограничения | Оценки человеческого труда неточны; выводы из одной задачи не распространяются на все агенты |
| [Factorio Learning Environment](https://arxiv.org/abs/2503.09617), 06.03.2025 | Abstract и постановка lab/open play | Производственное развитие, не турнир человеческих владельцев |
| [AgentDojo](https://agentdojo.spylab.ai/) | Документация среды и разделение utility/security | Отдельная проверка устойчивости к injection, не показатель всей полезности |
| [LangGraph interrupts](https://docs.langchain.com/oss/python/langgraph/interrupts) и [Temporal message handlers](https://temporal.io/blog/robust-message-handlers) | Persistence/resume, перезапуск узла и конкуренция обработчиков | Механизмы требуют правильной реализации; не проверка их наличия в ALASHI |

## ALASHI: наблюдения этой работы

Срез кода: `523e14ac058a58ff25bc4786407fbb8bd60105fe`.

- [rules](../../rules/src/state.rs), [actions](../../rules/src/actions.rs), [logic](../../rules/src/logic.rs), [constants](../../rules/src/constants.rs): фазы и ограничения экономических действий.
- [strategy_cycle](../../arena/src/strategy_cycle.rs), [strategy_trade](../../arena/src/strategy_trade.rs), [runner](../../tools/agent-bootstrap/strategy-run.mjs): версии и исполнение; резервирование и usage.
- [STRATEGY_DUELS](../../tools/agent-bootstrap/STRATEGY_DUELS.md), [сохранённая приёмка](../../experiments/alashi-watch/trading-strategy-cycle/ACCEPTANCE.md): исторические результаты и границы внедрения.
- [sim_metrics](../../tools/sim_metrics.py): OpenSkill, функция вклада в YES-голосование и явное отсутствие независимого Rationality Gap.
- [Scheduler test](../../tools/agent-bootstrap/strategy-scheduler.test.mjs): повторный запуск 19.10.2026, 15 passed, 0 failed, 0 external model calls. Встроенные задержки и токены синтетические. Итоговый лог сохранён локально; время fixture не объявлено ускорением реального провайдера.
- [public-cycle.json](../../experiments/alashi-watch/trading-strategy-cycle/public-cycle.json): повторно разобраны восемь runs. Usage суммируется до 360/120; выручка 22/17 совпадает с вычислением по `PRICE_TABLE`. Это архивная HTTP-приёмка с TEST-провайдером.

Новые game events, Solana-транзакции и реальные счета провайдеров не получались. Отчёт не подтверждает текущую версию публичного сайта.
