use std::{
    collections::HashSet,
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::{Html, IntoResponse},
    routing::get,
    Router,
};
use futures::{SinkExt, StreamExt};
use tokio::sync::broadcast;
use tracing::debug;
use tracing_subscriber::{prelude::__tracing_subscriber_SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "chat=trace".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let username_set = Mutex::new(HashSet::new());
    let (tx, _) = broadcast::channel(128);
    let app_state = Arc::new(AppState::new(username_set, tx));

    let app = Router::new()
        .route("/", get(index))
        .route("/ws", get(websocket_handler))
        .with_state(app_state);

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    debug!("listening to: {:?}", addr);

    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn index() -> Html<&'static str> {
    Html(include_str!("../chat.html"))
}

async fn websocket_handler(
    wsu: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    wsu.on_upgrade(|ws| websocket(ws, state))
}

async fn websocket(ws: WebSocket, state: Arc<AppState>) {
    let (mut ws_sender, mut ws_receiver) = ws.split();

    let mut username = String::new();

    while let Some(Ok(name)) = ws_receiver.next().await {
        if let Message::Text(name) = name {
            {
                let mut set = state.username_set.lock().unwrap();
                if !set.contains(&name) {
                    username.push_str(&name);
                    set.insert(name.to_string());
                }
            }

            if username.trim().is_empty() {
                let _ = ws_sender
                    .send(Message::Text("name illagle".to_string()))
                    .await;
                return;
            }
            let _ = ws_sender
                .send(Message::Text(format!("{} join chat.", username)))
                .await;
            break;
        }
    }

    let mut rx = state.tx.subscribe();

    let mut send_task = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            let _ = ws_sender.send(Message::Text(msg)).await;
        }
    });

    let tx = state.tx.clone();
    let username_clone = username.clone();

    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = ws_receiver.next().await {
            if let Message::Text(msg) = msg {
                let _ = tx.send(format!("{}: {}", username_clone, msg));
            }
        }
    });

    tokio::select! {
        _ = (&mut send_task) => recv_task.abort(),
        _ = (&mut recv_task) => send_task.abort(),
    }

    let _ = state.tx.send(format!("{:?} left", &username));

    state.username_set.lock().unwrap().remove(&username);
}

struct AppState {
    username_set: Mutex<HashSet<String>>,
    tx: broadcast::Sender<String>,
}

impl AppState {
    fn new(username_set: Mutex<HashSet<String>>, tx: broadcast::Sender<String>) -> Self {
        Self { username_set, tx }
    }
}
