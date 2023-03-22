//! Run with
//!
//!
//! ```not_rust
//! cargo run -p file-transfer
//! ```
//!
use std::{net::SocketAddr, sync::Arc};

use axum::{
    body::{self, Full},
    extract::{
        ws::{Message, WebSocket},
        Path, State, WebSocketUpgrade,
    },
    http::{header, HeaderValue, Response, StatusCode},
    response::{Html, IntoResponse},
    routing::get,
    Router,
};
use bytes::{Buf, Bytes};
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
        .route("/upload/*path", get(upload_file))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
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
    let tx = state.tx.clone();

    let mut recv = tokio::spawn(async move {
        while let Some(Ok(data)) = reader.next().await {
            if let Message::Binary(data) = data {
                // debug!("data: {:?}", data);
                if data.len() < 5 {
                    error!("data len less than 5");
                    break;
                }

                let mut b = Bytes::from(data);

                let _len = b.get_i32_le();
                let method_u8 = b.get_u8();
                // debug!("len: {:?}", len);
                // debug!("method: {:?}", method_u8);
                match method_u8 {
                    1 => {
                        let _ = tx.send(());
                    }
                    2 => {
                        // 2: upload file, totallen-len-filename-len-filedata
                        let name_len = b.get_i32_le() as usize;

                        let name = String::from_utf8(b.split_to(name_len).to_vec()).unwrap();

                        let _data_len = b.get_i32_le();
                        // Create the file. `File` implements `AsyncWrite`.
                        let path = std::path::Path::new(UPLOAD_DIR).join(name);
                        let mut file = BufWriter::new(File::create(path).await.unwrap());

                        let len = b.remaining();
                        let bytes = b.take(len).into_inner().to_vec();
                        let u8_slice: &[u8] = bytes.as_slice();
                        let mut data_reader = BufReader::new(u8_slice);

                        // Copy the body into the file.
                        tokio::io::copy(&mut data_reader, &mut file).await.unwrap();

                        let _ = tx.send(());
                    }
                    _ => {}
                }
            }
        }
    });

    let mut send = tokio::spawn(async move {
        while let Ok(_) = rx.recv().await {
            let mut files = tokio::fs::read_dir(UPLOAD_DIR).await.unwrap();
            let mut resp = vec![];

            while let Ok(Some(file)) = files.next_entry().await {
                let name = file.file_name().into_string().unwrap();
                let name_bytes = name.clone().into_bytes();
                resp.extend(i32::to_le_bytes(name_bytes.len() as i32));
                resp.extend(name_bytes);
            }

            let mut resp_data = i32::to_le_bytes(resp.len() as i32).to_vec();
            resp_data.append(&mut resp);
            let _ = sender.send(Message::Binary(resp_data)).await;
        }
    });

    tokio::select! {
        _ = (&mut recv) => send.abort(),
        _ = (&mut send) => recv.abort(),
    }
}

async fn upload_file(Path(path): Path<String>) -> impl IntoResponse {
    let path = path.trim_start_matches('/');
    let mime_type = mime_guess::from_path(path).first_or_text_plain();

    let path = std::path::Path::new(UPLOAD_DIR).join(path);
    if let Ok(file) = tokio::fs::read(path).await {
        Response::builder()
            .status(StatusCode::OK)
            .header(
                header::CONTENT_TYPE,
                HeaderValue::from_str(mime_type.as_ref()).unwrap(),
            )
            .body(body::boxed(Full::from(file)))
            .unwrap()
    } else {
        Response::builder()
            .status(StatusCode::NO_CONTENT)
            .header(header::CONTENT_TYPE, "")
            .body(body::boxed(Full::default()))
            .unwrap()
    }
}

struct AppState {
    tx: broadcast::Sender<()>,
}

impl AppState {
    fn new() -> Self {
        let (tx, _) = broadcast::channel(128);
        AppState { tx }
    }
}
