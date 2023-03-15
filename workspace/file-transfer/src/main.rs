//! Run with
//!
//!
//! ```not_rust
//! cargo run -p file-transfer
//! ```
//!
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
use tokio::{
    fs::File,
    io::{BufReader, BufWriter},
    sync::broadcast,
};
use tracing::{debug, error};
use tracing_subscriber::{prelude::__tracing_subscriber_SubscriberExt, util::SubscriberInitExt};

const UPLOAD_DIR: &str = "upload";

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "file_transfer=trace".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let _ = tokio::fs::create_dir_all(UPLOAD_DIR).await;

    let state = Arc::new(AppState::new());

    // state.files.lock().unwrap().push(value);

    let app = Router::new()
        .route("/", get(index))
        .route("/ws", get(websocket_handler))
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    debug!("listening on {:?}", &addr);

    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn index() -> Html<&'static str> {
    Html(include_str!("../file.html"))
}

async fn websocket_handler(
    wsu: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    wsu.on_upgrade(|ws| websocket(ws, state))
}

async fn websocket(ws: WebSocket, state: Arc<AppState>) {
    let (mut sender, mut reader) = ws.split();

    let mut rx = state.tx.subscribe();

    let mut recv = tokio::spawn(async move {
        while let Some(Ok(data)) = reader.next().await {
            if let Message::Binary(data) = data {
                // debug!("data: {:?}", data);
                if data.len() < 5 {
                    error!("data len less than 5");
                    break;
                }
                let mut idx = 0;

                let len = i32::from_le_bytes(data[idx..4].try_into().unwrap());
                idx += 4;
                let method_u8 = data[idx];
                idx += 1;
                // debug!("len: {:?}", len);
                debug!("method: {:?}", method_u8);
                match method_u8 {
                    1 => {
                        // 1: get file name list
                        let mut resp = vec![];
                        {
                            let name_arr = state.files.lock().unwrap();
                            for name in name_arr.iter() {
                                let name_bytes = name.clone().into_bytes();
                                resp.extend(i32::to_le_bytes(name_bytes.len() as i32));
                                resp.extend(name_bytes);
                            }
                        }
                        let mut resp_data = i32::to_le_bytes(resp.len() as i32).to_vec();
                        resp_data.append(&mut resp);
                        let _ = sender.send(Message::Binary(resp_data)).await;
                    }
                    2 => {
                        // 2: upload file, totallen-len-filename-len-filedata
                        let name_len = i32::from_le_bytes(data[idx..idx + 4].try_into().unwrap());
                        idx += 4;

                        let name = String::from_utf8(data[idx..(idx + name_len as usize)].to_vec())
                            .unwrap();
                        idx += name_len as usize;

                        let _data_len = i32::from_le_bytes(data[idx..idx + 4].try_into().unwrap());
                        debug!("data len: {:?}, {:?}", _data_len, data.len());
                        idx += 4;
                        debug!("data idx... len {:?}", &data[idx..(idx + 10)]);
                        // Create the file. `File` implements `AsyncWrite`.
                        let path = std::path::Path::new(UPLOAD_DIR).join(name);
                        let mut file = BufWriter::new(File::create(path).await.unwrap());
                        let mut data_reader = BufReader::new(&data[idx..]);

                        // Copy the body into the file.
                        tokio::io::copy(&mut data_reader, &mut file).await.unwrap();
                    }
                    _ => {}
                }
            }
        }
    });

    let mut send = tokio::spawn(async move {
        while let Ok(data) = rx.recv().await {
            debug!("send: {:?}", data);
        }
    });

    tokio::select! {
        _ = (&mut recv) => send.abort(),
        _ = (&mut send) => recv.abort(),
    }
}

struct AppState {
    files: Mutex<Vec<String>>,
    tx: broadcast::Sender<Vec<u8>>,
}

impl AppState {
    fn new() -> Self {
        let (tx, _) = broadcast::channel(128);
        AppState {
            files: Mutex::new(vec![]),
            tx,
        }
    }
}
