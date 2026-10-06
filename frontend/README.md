# alashi frontend

React + TypeScript + Vite, сцена на React Three Fiber + Drei, стор Zustand, данные арены через TanStack Query, стили Tailwind 4.

```bash
npm ci
npm run dev        # http://localhost:5173
npm run check:pose
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
