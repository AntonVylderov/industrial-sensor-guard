use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use include_dir::{include_dir, Dir};
use std::sync::Arc;
use tokio::sync::broadcast;
use serde_json::json;
use crate::models::{SensorReading, AggregatedReading};

static STATIC_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/web");

pub async fn run_web_dashboard(
    mut rx_raw: broadcast::Receiver<SensorReading>,
    mut rx_agg: broadcast::Receiver<AggregatedReading>,
    addr: &str,
) {
    tracing::info!("Starting web dashboard on {}", addr);
    let (ws_tx, _) = broadcast::channel::<String>(256);
    let ws_tx = Arc::new(ws_tx);

    // Форвардер сырых данных
    let ws_tx_raw = ws_tx.clone();
    tokio::spawn(async move {
        tracing::info!("WebSocket raw forwarder started");
        loop {
            match rx_raw.recv().await {
                Ok(reading) => {
                    let data = json!({
                        "type": "raw",
                        "parameter": reading.parameter,
                        "value": reading.value,
                        "unit": reading.unit,
                        "timestamp": reading.timestamp,
                    });
                    // send() возвращает Err только если нет подписчиков — это нормально
                    let _ = ws_tx_raw.send(data.to_string());
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("Raw forwarder lagged, skipped {} messages", n);
                }
                Err(broadcast::error::RecvError::Closed) => {
                    tracing::warn!("Raw channel closed");
                    break;
                }
            }
        }
    });

    // Форвардер агрегированных данных
    let ws_tx_agg = ws_tx.clone();
    tokio::spawn(async move {
        tracing::info!("WebSocket agg forwarder started");
        loop {
            match rx_agg.recv().await {
                Ok(agg) => {
                    let data = json!({
                        "type": "agg",
                        "parameter": agg.parameter,
                        "value": agg.value,
                        "unit": agg.unit,
                        "timestamp": agg.timestamp,
                    });
                    let _ = ws_tx_agg.send(data.to_string());
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("Agg forwarder lagged, skipped {} messages", n);
                }
                Err(broadcast::error::RecvError::Closed) => {
                    tracing::warn!("Agg channel closed");
                    break;
                }
            }
        }
    });

    let app = Router::new()
        .route("/ws", get(move |ws| handle_ws(ws, ws_tx.clone())))
        .route("/", get(serve_index))
        .route("/{*path}", get(serve_static));

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    tracing::info!("Web dashboard listening on http://{}", addr);
    axum::serve(listener, app).await.unwrap();
}

async fn handle_ws(ws: WebSocketUpgrade, tx: Arc<broadcast::Sender<String>>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, tx))
}

async fn handle_socket(mut socket: WebSocket, tx: Arc<broadcast::Sender<String>>) {
    let mut rx = tx.subscribe();
    tracing::info!("New WebSocket client connected");
    loop {
        match rx.recv().await {
            Ok(msg) => {
                if socket.send(Message::Text(msg.into())).await.is_err() {
                    tracing::info!("WebSocket client disconnected");
                    break;
                }
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                tracing::warn!("WebSocket client lagged, skipped {} messages", n);
            }
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }
}

async fn serve_index() -> Response {
    serve_file("index.html")
}

async fn serve_static(axum::extract::Path(path): axum::extract::Path<String>) -> Response {
    serve_file(&path)
}

fn serve_file(path: &str) -> Response {
    // Убираем ведущий слеш если есть
    let path = path.trim_start_matches('/');

    match STATIC_DIR.get_file(path) {
        Some(file) => {
            let mime = mime_for(path);
            let contents = file.contents();
            (
                axum::http::StatusCode::OK,
                [(axum::http::header::CONTENT_TYPE, mime)],
                contents,
            )
                .into_response()
        }
        None => {
            tracing::warn!("Static file not found: {}", path);
            (
                axum::http::StatusCode::NOT_FOUND,
                format!("Not found: {}", path),
            )
                .into_response()
        }
    }
}

fn mime_for(path: &str) -> &'static str {
    match path.rsplit('.').next() {
        Some("html") => "text/html; charset=utf-8",
        Some("css")  => "text/css; charset=utf-8",
        Some("js")   => "application/javascript; charset=utf-8",
        Some("svg")  => "image/svg+xml",
        Some("png")  => "image/png",
        Some("ico")  => "image/x-icon",
        Some("json") => "application/json",
        _            => "application/octet-stream",
    }
}