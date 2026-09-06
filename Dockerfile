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
WORKDIR /app
ENV ALASHI_SEQ_FILE=/data/arena_party_no.txt
EXPOSE 8090
CMD ["./arenad", "--port", "8090", "--bind", "0.0.0.0"]
