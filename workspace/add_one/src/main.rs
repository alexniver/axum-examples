use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use axum::{
    extract::{
        ws::{Message, WebSocket},
        State, WebSocketUpgrade,
    },
    response::{Html, IntoResponse},
    routing::get,
    Router,
};
use futures::{SinkExt, StreamExt};
use tokio::sync::broadcast;
use tracing::debug;
use tracing_subscriber::{prelude::__tracing_subscriber_SubscriberExt, util::SubscriberInitExt};

struct AppState {
    num: Mutex<i32>,
    tx: broadcast::Sender<i32>,
}

impl AppState {
    fn new(num: Mutex<i32>, tx: broadcast::Sender<i32>) -> Self {
        Self { num, tx }
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "add_one=trace".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let num = Mutex::new(1);
    let (tx, _) = broadcast::channel(100);

    let app_state = Arc::new(AppState::new(num, tx));

    let app = Router::new()
        .route("/", get(index))
        .route("/ws", get(ws))
        .with_state(app_state);

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    debug!("listening on {}", addr);

    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn ws(ws: WebSocketUpgrade, State(state): State<Arc<AppState>>) -> impl IntoResponse {
    ws.on_upgrade(|ws| websocket(ws, state))
}

async fn websocket(stream: WebSocket, state: Arc<AppState>) {
    let (mut sender, mut receiver) = stream.split();

    let mut rx = state.tx.subscribe();

    let _ = state.tx.send(*state.num.lock().unwrap());
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(_)) = receiver.next().await {
            let mut num = state.num.lock().unwrap();
            *num += 1;
            let _ = state.tx.send(*num);
        }
    });

    let mut send_task = tokio::spawn(async move {
        while let Ok(num) = rx.recv().await {
            if sender.send(Message::Text(num.to_string())).await.is_err() {
                break;
            }
        }
    });

    tokio::select! {
        _ = (&mut send_task) => recv_task.abort(),
        _ = (&mut recv_task) => send_task.abort(),
    }

    debug!("client quit");
}

async fn index() -> Html<&'static str> {
    Html(std::include_str!("../add.html"))
}
