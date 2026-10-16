# Alashi: два штаба в реальном времени

Локальный опыт повторного использования agentenv_rts из Warcraft-плагина. Два сценарных клиента управляют восемью Degenie через исходный RemoteSession. Часы поручений независимы от клиентов; Timeline сохраняет события.

Запуск из этой папки: `python3 runtime.py`. Адрес: http://127.0.0.1:8767. Проверка: `python3 check_runtime.py`. Используется стандартная библиотека Python; установка всего Warcraft-стека не требуется.

Кнопка задержки останавливает новые решения Chief Б на пять секунд. Уже выданные поручения продолжаются. Новый цикл очищает все отчёты. Выбор Degenie показывает его крупно, сохраняя общую карту.

Данные демонстрационные. Модели и сервер Alashi не вызываются. Доставка отчёта не меняет баланс. Обоснование переноса: REUSE.md. Проверки: evidence/runtime-checks.json. Исходные лицензии: vendor/AGENTENV-LICENSE, vendor/three/LICENSE и vendor/meshoptimizer/LICENSE.md.

Upstream: earakely-scale/agentenv-wc3-plugin, Apache-2.0, commit 2376c509b6b5691858df2c9d4d1f79a7b1a877d1. Изменения в перенесённых RTS-модулях отсутствуют. runtime.py, scene.js, index.html и style.css являются локальным адаптером и видом Alashi.
