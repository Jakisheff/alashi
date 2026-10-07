# alashi frontend

React + TypeScript + Vite, SPA на TanStack Router (файловые маршруты `src/routes/`, `autoCodeSplitting`: каждая
страница — отдельный ленивый чанк), сцена на React Three Fiber + Drei, стор Zustand, данные арены через TanStack Query,
стили Tailwind 4.

Маршруты: `/` — Degenie и онбординг (`routes/index.tsx`), `/graph` — граф агентов (`routes/graph.tsx`),
`/log?game=<id>` — лог партии (`routes/log.tsx`). В dev `/game/*` и `/agents/*` проксируются на alashi.network (только GET).
Параметры адреса проверяются в `validateSearch` маршрута. `src/routeTree.gen.ts` генерирует плагин при dev/build,
файл в Git (нужен `tsc -b` до сборки). На сервере nginx должен отдавать `index.html` на неизвестные пути
(`try_files $uri $uri/ /index.html`), иначе прямая ссылка на `/graph` даст 404.

```bash
npm ci
npm run dev        # http://localhost:5173
npm run check:pose
npm run check:events
npm run check:graph
npm run check:log
npm run typecheck
npm run lint
npm run build
```

## DeskGenie

Персонаж — `public/models/desk-genie.glb` (сцена: `src/genie/Scene.tsx`, грузится лениво; 60 fps максимум, пауза вне экрана) (риг + клипы `idle`, `act`, `accepted`, `rejected`),
грузится в `src/genie/GenieModel.tsx` через `useGLTF` и `useAnimations`. Клипы: кнопки внизу или клавиши 1–4.

- `?pose=accepted&t=0.55` замораживает позу: кадры для ревью и скриншотов.
- `npm run check:pose` проверяет, что клипы начинаются и заканчиваются в позе idle.
- Реплики Degenie печатаются в нижней полосе его экрана (`src/genie/ScreenText.tsx`, `useScene().say(text)`);
  длинный текст разбивается на страницы по две строки. Шрифт Jura (OFL, `public/fonts`).

Пересборка GLB (Blender 5.2):

1. `npm run export:poses` — сэмплирует `src/genie/pose.ts` в `art/poses.json` (30 fps).
2. В Blender выполнить `art/desk_genie.py` и вызвать `main("export")` (через Blender MCP или Text Editor).
   Скрипт строит модель, риг, клипы и пишет сырой `art/desk-genie.glb`; рендер-превью — в `art/renders/`.
3. `npm run optimize:glb` — meshopt-сжатие (нормали 12 бит) в `public/models/desk-genie.glb`: 1,58 МБ → ~0,41 МБ.
   `useGLTF` декодирует meshopt сам; скрипт падает, если пропали `text-anchor` или клипы.

## Арена

`src/events.ts` — курсор, дедупликация и правило пропуска по контракту `PUBLIC_EVENTS_FOR_DIN` (docs a3d8ed8);
`src/feed.ts` — лента для Degenie: каждое событие печатается на экране, затем `act` → `accepted`/`rejected`.

- `?game=<id>` — живая партия через `GET /game/<id>/state` (same-origin; опрос 2 с, пауза в скрытой вкладке). `id` — только целое > 0.
- `?api=<base>` — другая арена, **только в `npm run dev`**: в сборке параметр игнорируется, иначе любая ссылка показывала бы чужие тексты на alashi.network.
- Без параметров — постановочный поток, помеченный в HUD как PREVIEW «sample events, not a real game».

## Граф агентов

Маршрут `/graph` (`routes/graph.tsx` → `src/graph/`): кто с кем торгует, даёт взятки и покупает голоса.
Движок — порт D3-графа HackAlem `dai-front` (`money-graph.ts`, `graph-settings`, `graph-timeline`, 67d9d60):
узел — агент, кластер — партия, ребро — сделки агент → агент, таймлапс — раунды.

- Сейчас данные мок (`src/graph/mock.ts`, сид фиксирован), страница помечена MOCK DATA. Контракт данных для бэкенда —
  `src/graph/types.ts`; публичные события арены пока не содержат адресатов и сумм, поэтому живых данных нет.
- `?focus=<agent_record_id>` открывает окружение агента, `?agent=<id>` обводит «вашего агента».
- Отрисовка на Canvas (узлы, связи, оболочки партий, подписи; пакетами по стилю), физика D3 в Web Worker
  (`src/graph/layout.worker.ts`). `?mock=large` — стресс-тест на ~2,5 тыс. агентов и ~8,5 тыс. связей.
- Пинч и колесо над графом всегда зумят граф, а не страницу (трекпад Mac присылает пинч как ctrl+wheel).
- `npm run check:graph` — инварианты мока (рёбра = принятые сделки, один президент на партию).

## Лог партии

`/log?game=<id>` (`src/log/`): лента как у биржи, новые строки сверху и подсвечиваются. Источник — `GET /game/:id/state`.

- Завершённая партия: регистрации агентов (транзакции Solana devnet со ссылкой на Explorer), фазы раундов, ходы
  с деньгами после хода и Δ, решения по законам (голоса, принят/отклонён), расчёт и выплаты. «▶ Replay» проигрывает партию.
- Живая партия: арена публикует только последние 12 действий без сумм и адресатов; строки копятся с момента открытия.
- Деньги игры симулированы; на блокчейне только регистрация. `npm run check:log` — модель ленты.
- Для сайта нужен точный маршрут `/log` в nginx (как у `/graph`).

