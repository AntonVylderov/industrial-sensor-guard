.PHONY: build test run clean docker-build docker-run

build:   ## Собрать релизную версию
	cargo build --release

test:    ## Запустить все тесты
	cargo test --release

run:     ## Запустить локально с JSON-логами
	RUST_LOG=info cargo run

clean:   ## Очистить артефакты сборки
	cargo clean

docker-build: ## Собрать Docker-образ
	docker build -t industrial-sensor-guard .

docker-run:   ## Запустить контейнер
	docker run --rm industrial-sensor-guard