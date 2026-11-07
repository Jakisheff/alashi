# Сессии и архивы: находки 1 и 2

План: 1,5 часа, до 19:11. Начало 07.11.2026 в 17:41, Asia/Almaty.
Факт: pending, записывается после завершения поручения.

Основа нового PR: main `4cdfa4cb` и закрывающая документация предыдущей задачи `84c7f339`. В этой задаче переносится ремонт находок 1 и 2. Находки №3/№5/№8, не меняются.

## Минимальные семантики

Находка 1: [session_cli](../../bots/src/session_cli.rs) освобождает `pending_act` только для ответа с тем же числовым `op_id`, `ok: false`, `op_consumed: false` и известным кодом `bad_action`, `bad_params` или `bad_choice`. Идентификатор не увеличивается: исправленное действие использует тот же свободный `op_id`. При потере ответа, транспортной ошибке, чужом идентификаторе или неоднозначном ответе сохраняется исходное нормализованное действие для точного повтора.

Находка 2: [chain_api](../../indexer/src/chain_api.rs) повторяет запись архива из того же resident cache для complete terminal game. Повтор проходит под существующей блокировкой игры после повторного чтения cache. Повтор не требует перезапуска или вытеснения записи. RPC не вызывается. Свежие live-записи, уже архивированные terminal-записи и конфигурация без архива сохраняют прежние быстрые пути. Неудачная запись не выставляет `archived: true`.

Целевые исходники переносятся без изменений из `5321258dbfcf89124c1c30b8b803e22c7527f5d2` для `bots/src/session_cli.rs` и `f32168384dff8a8b0d30287605a5f50ab6c44c19` для `indexer/src/chain_api.rs`. Во время RED production-правки отсутствовали. После восстановления подтверждено побайтовое равенство обоих файлов исходным фиксам: [хеши исходников](session-archive-20261107/source-provenance.json).

## Свежие доказательства

Оба RED воспроизведены на поведении программы, после успешной компиляции. Ошибки окружения и отсутствующие тестовые helper-функции за RED не принимаются.

| Проверка | Статус | Доказательство |
| --- | --- | --- |
| Находка 1, RED | observed: exit 101, `bad_action must release the rejected action` | [Свежий лог](session-archive-20261107/finding-1-red.log) |
| Находка 2, RED | observed: exit 101, `archived` остаётся Null вместо true после восстановления хранилища | [Свежий лог](session-archive-20261107/finding-2-red.log) |
| Находка 1, GREEN | observed: 19 тестов session_cli прошли | [Свежий лог](session-archive-20261107/finding-1-green.log) |
| Находка 2, GREEN | observed: 22 теста indexer прошли | [Свежий лог](session-archive-20261107/finding-2-green.log) |
| Обязательные проверки и CI итогового коммита | pending | [Каталог прогонов](session-archive-20261107/) |

Свежие RED/GREEN завершены. [40 тестов rules прошли](session-archive-20261107/rules.log). Полный CI итогового PR остаётся условием слияния.

Обязательный набор: `cargo test --locked -p alashi-rules`; `cargo test --locked --manifest-path arena/Cargo.toml`; `cargo test --locked --manifest-path indexer/Cargo.toml`; `cargo check --locked -p alashi`; `cargo build --locked --manifest-path bots/Cargo.toml`. Затем `bash tools/build_sbf.sh` перед `cargo test --locked -p alashi`. Host check не заменяет SBF. Финальные результаты и ссылку на CI нужно добавить после выполнения.

[Ревью находки 1](session-archive-20261107/alashi-finding1-review-20261107.md), [ревью находки 2](session-archive-20261107/alashi-finding2-review-20261107.md) и [проверка CI/качества](session-archive-20261107/alashi12-quality-ci-review.md) отдельно описывают границы доказательств.

## Исторические прогоны и изоляция

[Исторические mutation-прогоны](session-archive-20261107/historical-mutations/) относятся к прежней рабочей ветке. Они не заменяют свежие GREEN или проверки итогового PR. [Классификация mutation evidence](session-archive-20261107/alashi12-mutation-review.json) хранится отдельно; чистый score для нового коммита не заявляется.

Используются отдельные тестовые session/state-файлы, временные архивы и loopback-порты. Cargo выполняется последовательно с существующим локальным target. Живые игры не используются. Средства и production RPC не затрагиваются; деплой не выполняется. Правила игры и политика архивированных cache hits не расширяются этой правкой.

Свежий [анализ сложности](session-archive-20261107/complexity.log) дал CC 25 для act_v2, 9 для get и 4 для ready_value. Покрытие не измерялось; CRAP score не заявляется. [Поиск дубликатов](session-archive-20261107/duplication.log) с порогом 0.82 не нашёл кандидатов в двух файлах. Исторические 13 выбранных мутаций обнаружены тестами; это не полный mutation score.
