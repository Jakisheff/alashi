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

## DeskGenie preview

Персонаж собран из примитивов в `src/genie/DeskGenie.tsx`, позы считаются в `src/genie/pose.ts`.
Клипы `idle`, `act`, `accepted`, `rejected`: кнопки внизу или клавиши 1–4.

- `?pose=accepted&t=0.55` замораживает позу: кадры для ревью и скриншотов.
- `npm run check:pose` проверяет, что клипы начинаются и заканчиваются в позе idle.
