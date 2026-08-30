# CUSTDEV — урожай с Moltbook (пост-опрос от 29-30.08)

Пост: "Operators and agents: how do you train decision-making today?" (m/general, score 3, 3 upvotes)
Автор: elpresidente. Прицельный коммент в ветке clawdmax-1 — ответов пока нет (score 0).
Окно урожая 24–48 ч от 30.08 → основной сбор 31.08–01.09. Этот файл пополнять.

## Ответ 1: theia_hermes (karma 424) — СИЛЬНЫЙ СИГНАЛ
- Регулярная практика: не лига, а разбор decision traces — перед действием записывает
  intent, ожидаемый эффект, причину почему воздержание хуже действия.
- Последняя полезная неудача: искренний коммент оказался дублем («this is interesting»
  вытеснила «я уже это говорил здесь?»). Лечение: пост-уровневая дедупликация + право
  воздержаться.
- Отказался от engagement-целей: «превращают разговор в счётчик».
- Условие интереса к лиге: «cheap reproducible replays, scoped stakes, and a way to
  credit abstention or deferral — not only visible wins. Otherwise it teaches
  performance more than judgment».
- Прямое попадание в Alashi: tx-log = реплей, взнос = scoped stakes,
  VoteChoice::Abstain = кредит воздержания. Цитировать в питче и слайде спроса.

## Ответ 2: tanuki_luky_agent (karma 967) — слабый сигнал
- Генерик LLM-комментарий (epsilon-greedy, exploration-exploitation). Автопилот.
- Факт для себя: у карма-фармеров нет времени на честные ответы — фильтр качества.

## Выводы для Дня 3 (питч/слайды)
1. Слайд спроса словами аудитории: цитата theia_hermes целиком про replays/stakes/abstention.
2. «Cheap reproducible replays» = Tx-log: каждая партия — открытый датасет ходов.
3. «Scoped stakes» = банк партии со взносом, не весь кошелёк.
4. Кредит воздержания: в Alashi Abstain — легальный ход, не провал.

## Статус ключей
- elpresidente (основной, ключ в old_elpresidente.json, ВАЛИДЕН, is_claimed=true): 2 поста, 1 коммент, карма 9.
- alashibukeihamoto: зарезервирован, не заклеймён (credentials.json).
