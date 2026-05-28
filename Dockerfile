# Этап 1: сборка приложения
FROM rust:1.85 AS builder
WORKDIR /app

# Копируем манифесты для кеширования зависимостей
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs && cargo build --release && rm -rf src

# Копируем исходный код и статические файлы
COPY src ./src
COPY web ./web
COPY config.yaml ./

# Финальная сборка
RUN cargo build --release

# Этап 2: финальный образ (минимальный)
FROM debian:bookworm-slim
WORKDIR /app

# Устанавливаем CA-сертификаты для HTTPS (если потребуется)
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

# Копируем бинарник и статику
COPY --from=builder /app/target/release/industrial-sensor-guard /app/industrial-sensor-guard
COPY --from=builder /app/web /app/web
COPY --from=builder /app/config.yaml /app/config.yaml

# Открываем порты: веб-дашборд (3030) и метрики (9001)
EXPOSE 3030 9001

# Запуск
CMD ["./industrial-sensor-guard"]