# alashi frontend

React + TypeScript + Vite, SPA на TanStack Router (файловые маршруты `src/routes/`, `autoCodeSplitting`: каждая
страница — отдельный ленивый чанк), сцена на React Three Fiber + Drei, стор Zustand, данные арены через TanStack Query,
стили Tailwind 4.

Маршруты: `/` — Degenie и онбординг (`routes/index.tsx`), `/graph` — граф агентов (`routes/graph.tsx`).
Параметры адреса проверяются в `validateSearch` маршрута. `src/routeTree.gen.ts` генерирует плагин при dev/build,
файл в Git (нужен `tsc -b` до сборки). На сервере nginx должен отдавать `index.html` на неизвестные пути
(`try_files $uri $uri/ /index.html`), иначе прямая ссылка на `/graph` даст 404.

```bash
npm ci
npm run dev        # http://localhost:5173
npm run check:pose
npm run check:events
npm run check:graph
npm run typecheck
npm run lint
npm run build
```

## DeskGenie

Персонаж — `public/models/desk-genie.glb` (риг + клипы `idle`, `act`, `accepted`, `rejected`),
грузится в `src/genie/GenieModel.tsx` через `useGLTF` и `useAnimations`. Клипы: кнопки внизу или клавиши 1–4.

- `?pose=accepted&t=0.55` замораживает позу: кадры для ревью и скриншотов.
- `npm run check:pose` проверяет, что клипы начинаются и заканчиваются в позе idle.
- Реплики Degenie печатаются в нижней полосе его экрана (`src/genie/ScreenText.tsx`, `useScene().say(text)`);
  длинный текст разбивается на страницы по две строки. Шрифт Jura (OFL, `public/fonts`).

Пересборка GLB (Blender 5.2):

1. `npm run export:poses` — сэмплирует `src/genie/pose.ts` в `art/poses.json` (30 fps).
2. В Blender выполнить `art/desk_genie.py` и вызвать `main("export")` (через Blender MCP или Text Editor).
   Скрипт строит модель, риг, клипы и пишет `public/models/desk-genie.glb`; рендер-превью — в `art/renders/`.

## Арена

`src/events.ts` — курсор, дедупликация и правило пропуска по контракту `PUBLIC_EVENTS_FOR_DIN` (docs a3d8ed8);
`src/feed.ts` — лента для Degenie: каждое событие печатается на экране, затем `act` → `accepted`/`rejected`.

- `?api=<base>&game=<id>` — живая партия через `GET <base>/game/<id>/state` (same-origin; опрос 2 с, пауза в скрытой вкладке).
- Без параметров — постановочный поток, помеченный в HUD как PREVIEW «sample events, not a real game».

## Граф агентов

Маршрут `/graph` (`routes/graph.tsx` → `src/graph/`): кто с кем торгует, даёт взятки и покупает голоса.
Движок — порт D3-графа HackAlem `dai-front` (`money-graph.ts`, `graph-settings`, `graph-timeline`, 67d9d60):
узел — агент, кластер — партия, ребро — сделки агент → агент, таймлапс — раунды.

- Сейчас данные мок (`src/graph/mock.ts`, сид фиксирован), страница помечена MOCK DATA. Контракт данных для бэкенда —
  `src/graph/types.ts`; публичные события арены пока не содержат адресатов и сумм, поэтому живых данных нет.
- `?focus=<agent_record_id>` открывает окружение агента, `?agent=<id>` обводит «вашего агента».
- `npm run check:graph` — инварианты мока (рёбра = принятые сделки, один президент на партию).

