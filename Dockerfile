# arenad на Fly.io: постоянный URL для демо, не зависит от ноутбука.
# Сборка на удалённом билдере Fly (локальный docker не нужен).
FROM rust:1.89-bookworm AS build
WORKDIR /src
COPY rules ./rules
COPY arena ./arena
RUN cargo build --locked --manifest-path arena/Cargo.toml --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/arena/target/release/arenad /app/arenad
COPY --from=build /src/arena/target/release/agent /app/agent
COPY app/arena.html /app/app/arena.html
# Аудит 27.09 (A2): процесс от ограниченного пользователя. Том /data
# должен быть доступен на запись этому uid (при деплое проверить
# владельца тома Fly).
RUN useradd --system --home-dir /app --shell /usr/sbin/nologin alashi \
    && chown -R alashi:alashi /app
USER alashi
WORKDIR /app
ENV ALASHI_SEQ_FILE=/data/arena_party_no.txt
ENV ALASHI_STATE_FILE=/data/arena_state.json
EXPOSE 8090
CMD ["./arenad", "--port", "8090", "--bind", "0.0.0.0"]
