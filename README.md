# Industrial Sensor Guard (2026 Standard)

[![CI](https://github.com/AntonVylderov/industrial-sensor-guard/actions/workflows/ci.yml/badge.svg)](https://github.com/AntonVylderov/industrial-sensor-guard/actions)
[![Rust 1.94+](https://img.shields.io/badge/rust-1.94+-orange.svg)](https://www.rust-lang.org)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Docker](https://img.shields.io/badge/docker-ready-blue)](https://hub.docker.com/)

Отказоустойчивый сервис мониторинга промышленных датчиков на Rust. Обнаруживает аномалии (значения > 85), логирует их в структурированный JSON и корректно завершает работу по Ctrl+C. Демонстрирует Senior‑уровень владения современным Rust: асинхронность, `thiserror`/`anyhow`, `tracing`, graceful shutdown.

## ✨ Ключевые особенности

- **Асинхронная архитектура** на `tokio` с каналом `mpsc` между Producer и Processor.
- **Обнаружение аномалий** — если значение > 85, выводится WARN с полным контекстом.
- **Observability** — все логи в JSON (готовы к Loki / Elasticsearch / CloudWatch).
- **Graceful shutdown** — корректное завершение по Ctrl+C, без потери отправленных данных.
- **Контейнеризация** — Docker‑образ собирается за секунды, работает на любой машине.
- **Полное покрытие тестами** — модульные тесты чистой логики + интеграционный тест.
- **CI/CD** — GitHub Actions автоматически запускает `cargo test` при каждом push.

## 🚀 Быстрый старт

### Локально (требуется Rust 1.94+)

```bash
git clone https://github.com/AntonVylderov/industrial-sensor-guard.git
cd industrial-sensor-guard
RUST_LOG=info cargo run

Через Docker
bash

docker build -t sensor-guard .
docker run --rm sensor-guard

Для выхода нажмите Ctrl+C — завершение будет аккуратным.
📊 Пример вывода
json

{"timestamp":"2026-05-26T10:15:30.123Z","level":"WARN","fields":{"sensor_id":"temp-001","value":91.2,"threshold":85.0,"timestamp":"2026-05-26 10:15:30.123Z"},"target":"processor","message":"ANOMALY DETECTED"}
{"timestamp":"2026-05-26T10:15:30.800Z","level":"INFO","fields":{"sensor_id":"vibro-003","value":72.5},"target":"processor","message":"Normal reading"}

🧱 Архитектура

    Producer — генерирует случайные показания датчиков (от 70.0 до 100.0) и отправляет их в канал.

    Processor — читает из канала, проверяет порог (85.0) и логирует результат.

    Main — инициализирует tracing (JSON-формат), запускает обе задачи и ожидает сигнал Ctrl+C для graceful shutdown.

Поток данных:
Producer → mpsc channel → Processor → JSON logs

    📘 Полное обоснование выбора технологий — Architecture Decision Records

🛠️ Стек технологий
Компонент	Крейты / инструменты
Асинхронность	tokio (mpsc, signal, spawn)
Обработка ошибок	thiserror (библиотечный слой), anyhow (main)
Логирование	tracing + tracing-subscriber (JSON формат)
Сериализация	serde, serde_json
Генерация данных	rand 0.9 (StdRng + ThreadRng)
Время	chrono (DateTime<Utc>)
Контейнеризация	Docker multi‑stage build
CI/CD	GitHub Actions
🧪 Тестирование
bash

cargo test

Проходят 4 теста:

    test_anomaly_detected — значение 85.1 считается аномалией.

    test_normal_value_not_anomaly — значение 85.0 не аномалия.

    test_boundary_exactly_threshold — граничное значение 85.0.

    test_processor_receives_and_completes — интеграционный тест процессора.

⚙️ CI/CD

GitHub Actions (.github/workflows/ci.yml) запускает:

    cargo fmt --check

    cargo clippy -- -D warnings

    cargo test --release

    cargo build --release

на каждом push в main и pull request.
📁 Структура проекта
text

src/
├── main.rs           # Инициализация, запуск задач, graceful shutdown
├── models.rs         # Структура SensorData (Serialize, Deserialize)
├── error.rs          # Определение ошибок (thiserror)
├── producer.rs       # Симулятор датчиков
└── processor.rs      # Анализатор аномалий (+ тесты)
Dockerfile            # Multi‑stage сборка контейнера
.github/workflows/    # CI конфигурация

🤔 Зачем этот проект?

Этот репозиторий создан как демонстрация Senior‑подхода к Rust-разработке:

    строгая обработка ошибок,

    асинхронная архитектура с backpressure,

    production‑grade observability,

    полная автоматизация CI/CD.

Идеально подходит как референс для собеседований или как основа реального сервиса на производстве.
📄 Лицензия

MIT — делайте что угодно, звёздочка на репозиторий приветствуется 😊


### docs/adr/0001-use-tokio-for-concurrency.md
Создайте эту папку и файл:

```markdown
# ADR 0001: Выбор Tokio в качестве асинхронного рантайма

**Дата:** 2026-05-26  
**Статус:** Принято

## Контекст
Нам нужен отказоустойчивый сервис мониторинга промышленных датчиков. Ожидается одновременная работа двух долгоживущих задач: симулятора (Producer) и анализатора (Processor). Они должны взаимодействовать асинхронно, не блокируя друг друга и не порождая избыточных потоков ОС.

## Решение
Выбран `tokio` с многопоточным рантаймом и каналом `mpsc`.  
- `tokio::spawn` позволяет запускать задачи, которые автоматически распределяются по потокам.  
- `mpsc::channel` даёт backpressure и естественное завершение при закрытии отправителя.  
- `tokio::signal::ctrl_c` используется для graceful shutdown без потери уже отправленных данных.

## Альтернативы
- **async-std** – менее распространён в production, меньшее сообщество и интеграций.  
- **Потоки ОС + std::sync::mpsc** – блокирующий приём не вписывается в асинхронную модель, усложнило бы обработку Ctrl+C.  
- **Actix** – избыточен, так как нам не нужен HTTP-сервер.

## Последствия
- Зависимость от экосистемы Tokio (стабильна с 1.x).  
- Простота тестирования: в тестах используем `#[tokio::test]`.  
- Код легко расширяется добавлением новых задач (например, персистентность логов).