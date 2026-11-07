# Аудит анимаций ALASHI, 29 октября 2026

Итог на 16:18 Алматы: пять исправлений опубликованы в `main-a4e85f3-20261029`. Проверки CI, публичных файлов и сцен в Chrome пройдены. Телефонные размеры проверялись в браузере; физический телефон не использовался. Результаты публикации приведены в конце документа, ниже сохранена запись проверки до объединения.

Статус на 29.10.2026 14:39 Алматы: подготовлено к финальному ревью PR #65. Коммит `75f1bd9` отправлен в рабочую ветку `codex/alashi-animation-audit-20261029`; все пять jobs CI run `40848875442` прошли. Визуальные результаты ниже относятся к локальной сборке и отмеченным исходным production-кадрам. Map replay ещё не просмотрен после исправления, публикация на alashi.network не подтверждена. Общий визуальный PASS не выставлен.

Критерии взяты из [канона Лассетера](../200_ZETTEL/20261010-lasseter-animation-canon.md). Проверяемый результат: подготовка персонажа читается до контакта с предметом, а зритель понимает последствие. Пауза и обратная перемотка сохраняют выбранный кадр. Последний баланс не становится балансом исторического кадра. Превью обозначено отдельно от подтверждённого события.

## Покрытие и способы просмотра

source_defined: найдено 18 режимов персонажа. Общий WebGL Canvas использует `desk-genie.glb`; отдельная whole-map ветка рисует запечённые 3D-спрайты через Canvas2D. `StudioStage` является декоратором, отдельного маршрута `/studio` нет.

| Вход | Библиотека и управление |
| --- | --- |
| `/` | Подтверждённый replay завершённой devnet-игры; выбор игрока, Pause/Play, Replay, Event slider. |
| `/?demo` | Пять legacy-клипов: кнопки и клавиши 1…5, переключатель подписей. |
| `/?pose=<clip>&t=<seconds>` | Замороженная legacy-поза; default `t=0.5`, время ограничено длительностью клипа. |
| `/?game=<HTTP id>` или `?agent=<record>` | Legacy по HTTP feed; mouse orbit/zoom. Triple-click за 900 ms вызывает `fuckOff`, кроме frozen mode. `?api=` работает только в DEV. |
| `/live` | Пять действий, семь реакций и living idle; Pause/Resume, scrub с шагом 0.01 s, replay, вход реквизита снизу/справа. У реакций есть кнопки ключевых кадров. |
| `/live`, prepared story | Conversation, Silence, Reconnect, Between games, B0 preview; 45 s, Restart, speed 1×/2×/5×, System events. Этот speed не меняет ручные клипы. |
| `/stream?agent=<64 hex>` | Публичные действия и явные gesture cues существующего агента; Reconnect, Copy link, Change agent. Ручных кнопок клипов нет. |
| `/devnet` | Living idle, пока не выбрана игра через Setup. |
| `/devnet?game=<GamePDA>&player=<FactionPDA>` | Подтверждённые действия, Replay/Pause/Latest, action slider; whole-map имеет отдельные Replay/Pause/seconds controls. |
| `/arena?game=<HTTP id>` | Canvas2D atlas по HTTP receipts; Reconnect и map replay. Деньги симулируются. |
| `/strategies?challenge=<hex>&agent=<hex>&parent=<hex>&mechanism=<id>` | Canvas2D atlas выбранного Duel; Replay/Pause/seconds slider. Query выбирает данные, не клип. |

`/graph` содержит анимированный 2D-граф без Degenie/WebGL. `/log`, `/about/` и `/devnet-runner.html` новых 3D-клипов не добавляют. Standalone-сцены в `experiments/alashi-watch/` не зарегистрированы как production SPA routes.

## Матрица 18 режимов

source_defined: длительности и смысловые кадры сверены с [legacy pose](../../frontend/src/genie/pose.ts), [reactions](../../frontend/src/live/reactions/definitions.ts), [actions](../../frontend/src/live/actions/definitions.ts), [purchase](../../frontend/src/live/market/purchase.ts) и [sale](../../frontend/src/live/market/sale.ts). Время в секундах. Таблица задаёт кадры проверки, а не общий визуальный PASS.

| Режим | Длительность | Намерение и ключевые кадры |
| --- | ---: | --- |
| Legacy idle | 4 | Hover и взгляд; blink 1.8→1.94; шов 0/4. |
| Legacy act | 1.1 | Подготовка 0.25; указание 0.45; hold 0.8; idle 1.1. Исправленная поза 0.45 просмотрена на desktop и 390px; кисть помещается в кадр. |
| Legacy accepted | 1.2 | Оседание 0.2; подъём 0.45; левая рука/кивок 0.55; восстановление 0.9→1.2. |
| Legacy rejected | 1.6 | Отскок 0.12; взгляд на руку 0.3→0.7; shrug 0.95; восстановление 1.2→1.6. |
| Legacy fuckOff | 5.8 | Подготовка кулаков 0.65/0.75; winding 1→3.9; reveal 1.65/2.3/3/3.9; hold 5; idle 5.8. |
| Living idle | 24 | 0/4/5.3/6.3/8.2/14.8/16.3/19/24: глаза, левый поворот, наклон, центр, правый поворот, центр, шов. |
| Thumbs up | 5.2 | 0/0.45/1.25/1.9/2.65/3.8/5.2: подготовка, палец вверх, кивок, hold, release, idle. |
| Realization | 5.2 | 0/0.45/1.3/2.3/3.1/3.9/5.2: recoil, ладони снаружи висков, поклон, shake, recovery. |
| Facepalm | 5.2 | 0/0.5/1.45/2.25/3.1/3.9/5.2: вздох, ладонь перед лицом, поклон, hold, withdrawal, idle. |
| Thinking | 5.2 | 0/0.6/1.5/2.65/3.9/5.2: взгляд, наклон, кулак под подбородком, кивок, withdrawal. |
| Shrug | 5.2 | 0/0.5/1.5/2.65/3.9/5.2: подготовка, ладони вверх с задержкой второй руки, tilt, settle. |
| Look around | 5.2 | 0/0.8/1.8/2.8/3.9/5.2: левый взгляд, правый взгляд, lean, центр. |
| Victory | 5.2 | 0/0.5/1.25/2.3/3.1/3.9/5.2: windup, кулаки вверх, celebrate, proud nod, settle. |
| Buy | 6.4 | Взгляд 0.25→0.65; torso 0.65→1.05; оплата 1.3→2.35; ящик 2.7→3.65; вес 3.65/3.8; hold 4.25; fade 4.9→5.5; idle 6.4. |
| Sell | 6.4 | Взгляд 0.25→0.65; lift 1.45→1.75; offer 2.15→2.8; передача 2.8→3.25; монета 3→3.65; wink 3.85→4.4; settle 5.4→6.4. |
| Mule | 7.2 | Glances 0.8/1.4; coin 1.9; оплата 2→2.38; parcel 2.8→3.3; catch dip 3.55; hold 4.3; wink 4.65→5.12; idle 7.2. |
| Bribe | 7.2 | Look around 1.35/2/2.7; offer 3.65; контакт 3.7→4.05; withdrawal 4.5→5.2; paperwork 5.4; idle 7.2. |
| Vote | 7.2 | Lift 1.5; turn 2.8; hold 3.35; release 3.5; fall 3.75; inside 4.1; relax 5.3; idle 7.2. |

source_defined: GLB содержит все пять legacy-клипов и дополнительный export frame: 4.033333/1.133333/1.233333/1.633333/5.833333 s соответственно. `market-sale-props.glb` не содержит клипов. Atlas имеет idle/act/accepted, восемь направлений и 24 кадра на направление; длительности 4.033/1.133/1.233 s.

Reduced-motion playback удерживает смысловую позу, затем возвращается к idle. Legacy кадры: 0/0.45/0.55/0.95/3.9. Реакции: thumbsUp 2.65, realization 2.3, facepalm 2.25, thinking 2.65, shrug 2.65, lookout 2.8, victory 2.3. Actions: mule 3.65, bribe 3.7, vote 3.3; trade 4.25. Paused scrub остаётся точным.

FX проверяются отдельно: victory bursts 1.05/1.85/2.65 с жизнью 1.3 s; Buy sparks 3.65→4.3; purchase receipt 3.65→5.5; подтверждённая Sell-монета 3.65→4.70; platform pulse 400 ms. ScreenText печатает 32 символа/s, страница 46 символов, hold 1.8 s. Числа покупки в demo являются примерами.

## Применимость 11 принципов

source_defined: целевой облик и геометрия поз в этом исправлении сохранены. Ни одна строка ниже не заменяет просмотр сцены в движении.

| Принцип | Применение и текущая граница проверки |
| --- | --- |
| Squash and stretch | Жёсткий корпус сохраняет форму; усилие выражено наклоном и посадкой. Новая деформация корпуса не добавлена. |
| Timing | Длительности заданы выше. Исправлены независимые часы face text и atlas; accepted проходит один раз, затем idle. |
| Anticipation | Buy/Sell имеют взгляд до torso; у реакций и scenarios есть отдельная подготовка. Atlas теперь начинает act/accepted с первого кадра. |
| Staging | Лицо и контакт должны помещаться в кадр. Исправлены thinking-разметка `/stream` и non-studio framing. Численные bounds пройдены. После исправления просмотрены legacy-позы и все семь studio-реакций на desktop; телефонные проверки перечислены ниже. Физический телефон ещё не проверен. |
| Follow through and overlapping action | Tail запаздывает на 0.2 + i×0.035 s у реакций, 0.38 + i×0.035 s у scenarios. Профили и контакты не переписаны. |
| Straight ahead action and pose to pose | Смысловые позы и контакты перечислены в матрице. Каждый sample восстанавливает исходные transforms; процедурные детали дополняют позы. |
| Slow in and out | Сохранены smoothstep/easing существующих траекторий. Исправление часов не ускоряет клип accepted. |
| Arcs | Дуги переноса реквизита сохранены. IK вращает суставы без изменения длины костей. Контакты пяти действий просмотрены в телефонном размере; desktop-проверки перечислены ниже. |
| Secondary action | Мимика и дым сопровождают жест; sparks/fireworks подчинены действию и отключаются при reduced motion. |
| Exaggeration | Амплитуда поз сохранена, включая принятие веса при catch dip. Усиление поз или увеличение реквизита в этот проход не добавлены. |
| Appeal | У действий и реакций разные силуэты. Семь реакций просмотрены в телефонном размере и на desktop. Понимание жестов новым зрителем не проверялось. |

Personality связывает подготовку с результатом. Превью не является эмоцией реального агента. В потоке используются только опубликованные слова/cues; победа devnet требует payout rank 0 выбранного wallet. Новый эпизод игры или платный вызов для аудита не запускался.

## Подтверждённые дефекты

| Дефект | Исправление / статус |
| --- | --- |
| Face text продолжал печататься на паузе и расходился с reverse seek. | `ScreenText` получает часы preview; `screenTimeline` вычисляет кадр без накопления. Асинхронная речь stream имеет собственное время. Исправление pause проверено в браузере. |
| Atlas act/accepted начинался с глобальной фазы; chief повторял accepted бесконечно. | `atlasMotion` использует начало конкретного work/handoff; accepted 1.233 s однократный, replay tail 1.433 s включает idle. Stage timestamps и пути сохранены. |
| `?demo&game=` и `?pose=…&game=` получали live/final badge. | `homePresentation` отдаёт приоритет illustrative preview. Feed и маршруты сохранены. |
| `/stream` не включал существующее thinking-clearance CSS. | Добавлены `data-reaction` и общий `.live-scene` wrapper; bubble остаётся снаружи с прежним окном видимости. Проверены реальная JSX-разметка и локальная страница `/stream` с явным публичным thinking cue. На 390px длинное облако и сообщение на лице читаются. Живые события для проверки не создавались. |
| Legacy act при t=0.45 на desktop 1280 обрезает правый палец. | observed: исходный дефект виден в `legacy-act-desktop.jpg`. `cameraPlacement` задаёт non-studio factor 1.36; OrbitControls сохраняет нужную дистанцию на высоком canvas. Численно проверены все пять клипов. Исправленные legacy-позы помещаются в просмотренные desktop/mobile кадры; accepted на 320×560 не перекрывается подписью. Состав просмотра указан ниже. |

## Выполненные проверки и остаток

observed: ведущий агент просмотрел в браузере мобильные пять действий и семь реакций. Отдельно просмотрены тесные композиции на ширине 320 px. У новых studio-сцен проверка головы не потребовала изменения геометрии. Исправление face text pause проверено отдельно. В локальном `/stream` на 390px просмотрено длинное облако мысли вместе с сообщением на лице.

observed: после исправления камеры просмотрены act на desktop/390px, accepted на desktop/320×560 и fuckOff на desktop/390px. Accepted не перекрывается подписью и не вызывает горизонтальную прокрутку. Rejected на desktop просмотрен при 0.95 s; idle просмотрен в мобильном demo. Первый мобильный кадр rejected снят до завершения смены выражения, поэтому он не принят как проверка исправленной позы.

observed: все семь реакций на desktop проиграны от начала и остановлены на смысловом кадре: thumbsUp 2.65 s; realization 2.3 s; facepalm 2.25 s; thinking 2.65 s; shrug 2.65 s; lookout 2.8 s; victory 2.3 s. Голова и кисти помещаются в кадр. Victory возобновлён с 2.3 до 2.9 s, движение наблюдалось. Desktop-кадры Sell/Mule/Bribe просмотрены, проблем контакта не найдено. У Vote проверены release при 3.75 s и обратный seek к hold при 3.35 s через штатный slider: бюллетень возвращается в соответствующую позу.

observed: управление браузером вновь стало недоступно в 14:37. После этого новых визуальных результатов не добавлено. Map replay и публичный релиз остаются следующими проверками.

| Проверка | Текущий результат |
| --- | --- |
| CI | Run `40848875442`: frontend; SBF + full match; Rust; audit; gitleaks завершились успешно. Проверка относится к source commit `75f1bd9`. |
| Геометрия анимаций | 8654 выборки: legacy 827, reactions 1099, scenarios 5196, Buy contact 1532. Численные проверки пройдены. |
| Head-frustum | 8792 выборки головы пройдены. Это не проверка всего силуэта: обрезанный палец legacy act найден отдельно. |
| Legacy framing | 827 кадров × 7 размеров canvas пройдены, включая 544×1366 с OrbitControls clamp. Проверены поля для badge: 38 px mobile / 42 px desktop. Studio placement сохранён в пяти численных случаях. |
| Новые регрессии | `map-animation-check.ts`: 6/6; `home-preview-check.ts`: 4/4; `screen-text-check.mjs` проверяет реальные caller bindings; `stream-staging-check.mjs` проверяет JSX без GPU. |
| Совместимость карты | `check:strategies`, `check:trading`, typecheck и scoped lint пройдены. Пути и reference assets сохранены. |
| Crapper | По LCOV новые atlas/home функции: CRAP 9/5/4/3, coverage 100%. Это локальные функции, не оценка всего frontend. |
| Dryer | Для новых atlas/home helper-функций дубликаты не найдены. |
| Mutator | Atlas/home: убиты 45 из 45 мутантов. Выживших 0; непокрытых точек 0. `screenTimeline`: 17/17 убиты. Результаты относятся к helper-регрессиям, не к художественной приёмке. |

not_validated: после исправлений ещё не просмотрены mobile rejected со settled-выражением и map replay. Проверка alashi.network после публикации остаётся открытой. Системный reduced motion в браузере не переключался; численные проверки reduced-motion поз не заменяют этот просмотр. Физический телефон не использовался, понимание жестов новым зрителем не проверялось. Телефонный размер означает viewport настольного браузера. Общий визуальный PASS не выставлен.

## Сохранённые кадры

observed: мобильные контакты [Buy](../assets/animation-audit-20261029/buy-catch-mobile.jpg), [Sell](../assets/animation-audit-20261029/sell-payment-mobile.jpg), [Mule](../assets/animation-audit-20261029/mule-catch-mobile.jpg), [Bribe](../assets/animation-audit-20261029/bribe-contact-mobile.jpg) и [Vote](../assets/animation-audit-20261029/vote-release-mobile.jpg) сняты на опубликованной версии до исправлений. [Thinking](../assets/animation-audit-20261029/thinking-mobile.jpg) показывает целевой отступ облака в demo.

[Обрезанная кисть act](../assets/animation-audit-20261029/legacy-act-desktop.jpg) и [голова под подписью accepted](../assets/animation-audit-20261029/legacy-accepted-mobile.jpg) фиксируют дефекты старой камеры. [Пауза текста после исправления](../assets/animation-audit-20261029/fixed-thinking-paused-2.jpg) проверена на локальной сборке. Они показывают исходные дефекты и отдельную проверку паузы.

observed: после исправления сняты [act на desktop](../assets/animation-audit-20261029/fixed-legacy-act-desktop.jpg) и [act на 390px](../assets/animation-audit-20261029/fixed-legacy-act-mobile.jpg), [accepted на 320×560](../assets/animation-audit-20261029/fixed-legacy-accepted-320.jpg), [fuckOff на 390px](../assets/animation-audit-20261029/fixed-legacy-fuckoff-mobile.jpg). Кисти помещаются в кадр, лицо accepted не перекрывается подписью. [Thinking в настоящем компоненте `/stream`](../assets/animation-audit-20261029/fixed-stream-thinking-mobile.jpg) использует помеченные LOCAL QA события локальной фикстуры. Этот кадр не является записью реального матча.

observed: с дополнительным CSP локальной фикстуры возникала ошибка worker. Дополнительное разрешение unsafe-eval не устранило сбой; искусственный заголовок убран из тестового сервера. Production CSP не менялся. Визуальная проверка локального `/stream` выполнена после удаления этого ограничения.

observed: desktop-кадры [Thinking](../assets/animation-audit-20261029/fixed-thinking-desktop.jpg) и [Facepalm](../assets/animation-audit-20261029/fixed-facepalm-desktop.jpg) сохраняют лицо и кисти в кадре. У Vote зафиксированы [release 3.75 s](../assets/animation-audit-20261029/fixed-vote-desktop.jpg) и [reverse seek 3.35 s](../assets/animation-audit-20261029/fixed-vote-desktop-reverse.jpg). Это локальная сборка, не подтверждение публичного релиза.

## Публикация и дополнительные проверки

Статус на 29.10.2026 16:15 Алматы. Запись выше сохраняет состояние до объединения PR #65; этот раздел добавляет последующие результаты.

observed: PR #65 объединён в `main` коммитом `a4e85f379e9cdb295dad0dccea178949c14df13f`. Все пять jobs PR CI `40851777684` и main CI `40852936635` прошли. Опубликован frontend release `main-a4e85f3-20261029`. На сервере сохранены четыре PID сервисов, хэши обоих исполняемых файлов не изменились. Candidate guard сохранил 60 файлов deck и четыре downloads. Данные проверки записаны в [отчёт публикации](DEPLOY_ANIMATION_AUDIT_20261029.json); GET-проверка опубликованной версии прошла: 120 из 120 immutable assets совпали, ошибок нет. Публичная проверка браузером ещё выполняется.

observed: повторно просмотрен [rejected на 390px при 0.95 s](../assets/animation-audit-20261029/fixed-legacy-rejected-mobile.jpg) после установления выражения лица; обе кисти помещаются в кадр. Сохранён [кадр проигрывания fuckOff в мобильном demo](../assets/animation-audit-20261029/fixed-legacy-fuckoff-mobile-motion.jpg). Также просмотрен [idle на desktop](../assets/animation-audit-20261029/fixed-legacy-idle-desktop.jpg). Пропуск mobile rejected из записи до объединения закрыт.

observed: настоящий компонент `/stream` показал короткую двухстрочную мысль на desktop; лицо и его сообщение читаются в [кадре](../assets/animation-audit-20261029/fixed-stream-thinking-desktop.jpg). Локальные события помечены LOCAL QA. Это проверка renderer с фикстурой, не новый эпизод реального агента.

observed: исправленная карта просмотрена через локальную архивную фикстуру завершённой devnet-игры с 72 событиями. Все четыре ответа этой фикстуры проверены реальным декодером и сравнением HTTP-байтов с сохранёнными данными. На desktop 1280px просмотрены [replay](../assets/animation-audit-20261029/fixed-map-desktop.jpg) и [перемотка 9.7→0 s](../assets/animation-audit-20261029/fixed-map-rewind-desktop.jpg). На 390px просмотрен [конец 33.5 s](../assets/animation-audit-20261029/fixed-map-mobile-end.jpg): scrollWidth 390 при innerWidth 390, горизонтального переполнения нет. Повторное проигрывание событий на мобильном viewport наблюдалось; сохранён [кадр движения](../assets/animation-audit-20261029/fixed-map-mobile-motion.jpg). Это закрывает отложенную локальную проверку map replay.

not_validated: визуальная проверка опубликованного release пока не завершена. Физический телефон не использовался; производительность на нём не измерялась. Browser reduced motion не переключался. Понимание жестов новым зрителем не проверялось. Общий визуальный PASS и итоговое время здесь не выставлены.

## Проверка публичной версии завершена

observed: ведущий агент завершил публичную проверку 29.10.2026 в 16:18 Алматы. Браузер загрузил script из `/releases/main-a4e85f3-20261029/`. На опубликованной карте desktop и mobile штатный slider остановлен на 9.7 s; позиции соответствуют локальной исправленной сцене. Запуск replay и последующая пауза просмотрены. Сохранён [публичный мобильный кадр карты](../assets/animation-audit-20261029/public-map-mobile.jpg).

observed: legacy act на публичной версии помещает кисть целиком на desktop и мобильном viewport; [desktop-кадр](../assets/animation-audit-20261029/public-fixed-legacy-act-desktop.jpg) сохраняет исправленный силуэт. В `/live` thinking остановлен на 0.6 s. Текст одинаков в двух мобильных кадрах с метками 15:04:16 и 16:15:18. Между ними подтверждён скачок часов, поэтому разница меток не является измеренной длительностью наблюдения. Ошибок JavaScript в просмотренных публичных сценах нет.

not_validated: производительность на физическом телефоне и переключение browser reduced motion не проверялись. Телефонные проверки выполнялись в viewport настольного браузера. Понимание жестов новым зрителем также не проверялось.

Факт: 3,7 часа по часам ноутбука, 12:34→16:18. В интервал входит подтверждённый скачок примерно на 71 минуту; это ненадёжная оценка активного времени работы. Исправления и публикация завершены. Последний план: ещё 0,25 часа, до 16:30, закрыт по результату проверки в 16:18.
