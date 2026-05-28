use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::response::{IntoResponse, Redirect};
use axum::routing::get;
use axum::{Json, Router};
use futures_util::{SinkExt, StreamExt};
use local_ip_address::local_ip;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tauri::Manager;
use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot, Mutex};
use tower_http::services::{ServeDir, ServeFile};
use uuid::Uuid;

const DEFAULT_PORT: u16 = 17860;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "kebab-case")]
enum ServiceStatus {
    Stopped,
    Starting,
    Running,
    Error,
}

#[derive(Debug, Clone, Serialize)]
struct ServerInfo {
    status: ServiceStatus,
    ip: String,
    url: String,
    error: Option<String>,
}

#[derive(Debug, Clone)]
struct ServerControl {
    status: ServiceStatus,
    ip: String,
    url: String,
    error: Option<String>,
    port: u16,
}

impl ServerControl {
    fn new(port: u16) -> Self {
        let ip = resolve_local_ip();
        Self {
            status: ServiceStatus::Stopped,
            url: format!("http://{ip}:{port}"),
            ip,
            error: None,
            port,
        }
    }

    fn to_info(&self) -> ServerInfo {
        ServerInfo {
            status: self.status.clone(),
            ip: self.ip.clone(),
            url: self.url.clone(),
            error: self.error.clone(),
        }
    }

    fn refresh_ip_url(&mut self) {
        self.ip = resolve_local_ip();
        self.url = format!("http://{}:{}", self.ip, self.port);
    }
}

struct ServerHandle {
    shutdown: oneshot::Sender<()>,
    join_handle: tauri::async_runtime::JoinHandle<()>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StudentSession {
    connection_id: String,
    nickname: String,
    connected: bool,
}

#[derive(Debug, Default)]
struct SessionHub {
    students: HashMap<String, StudentSession>,
    student_channels: HashMap<String, mpsc::UnboundedSender<String>>,
    teacher_channels: HashMap<String, mpsc::UnboundedSender<String>>,
    console_channels: HashMap<String, mpsc::UnboundedSender<String>>,
}

#[derive(Clone)]
struct HttpState {
    hub: Arc<Mutex<SessionHub>>,
}

struct BackendRuntime {
    control: ServerControl,
    hub: Arc<Mutex<SessionHub>>,
    running: Option<ServerHandle>,
}

#[derive(Clone)]
struct BackendState {
    inner: Arc<Mutex<BackendRuntime>>,
}

impl BackendState {
    fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(BackendRuntime {
                control: ServerControl::new(DEFAULT_PORT),
                hub: Arc::new(Mutex::new(SessionHub::default())),
                running: None,
            })),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SignalEnvelope {
    event: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    nickname: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    payload: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

#[derive(Deserialize)]
struct WsQuery {
    role: Option<String>,
}

async fn health() -> impl IntoResponse {
    Json(json!({ "ok": true }))
}

async fn student_page() -> impl IntoResponse {
    Redirect::temporary("/app/?mode=student")
}

async fn teacher_page() -> impl IntoResponse {
    Redirect::temporary("/app/?mode=teacher")
}

async fn ws_handler(
    Query(query): Query<WsQuery>,
    ws: WebSocketUpgrade,
    State(state): State<HttpState>,
) -> impl IntoResponse {
    let role = query.role.unwrap_or_else(|| "student".to_string());
    ws.on_upgrade(move |socket| handle_socket(socket, state.hub, role))
}

fn resolve_local_ip() -> String {
    local_ip()
        .map(|ip| ip.to_string())
        .unwrap_or_else(|_| "127.0.0.1".to_string())
}

fn send_json(tx: &mpsc::UnboundedSender<String>, envelope: &SignalEnvelope) {
    if let Ok(payload) = serde_json::to_string(envelope) {
        let _ = tx.send(payload);
    }
}

async fn broadcast_student_list(hub: &Arc<Mutex<SessionHub>>) {
    let mut guard = hub.lock().await;
    let students: Vec<StudentSession> = guard.students.values().cloned().collect();
    let payload = SignalEnvelope {
        event: "students".to_string(),
        source: None,
        target: None,
        nickname: None,
        payload: Some(json!({ "students": students })),
        message: None,
    };

    let Ok(raw) = serde_json::to_string(&payload) else {
        return;
    };

    let mut stale = Vec::new();
    for (id, tx) in &guard.teacher_channels {
        if tx.send(raw.clone()).is_err() {
            stale.push(id.clone());
        }
    }

    for (id, tx) in &guard.console_channels {
        if tx.send(raw.clone()).is_err() {
            stale.push(id.clone());
        }
    }

    for id in stale {
        guard.teacher_channels.remove(&id);
        guard.console_channels.remove(&id);
    }
}

async fn forward_signal(
    hub: &Arc<Mutex<SessionHub>>,
    fallback_teacher: Option<String>,
    source_id: Option<String>,
    mut envelope: SignalEnvelope,
    sender: &mpsc::UnboundedSender<String>,
) {
    let target_id = envelope
        .target
        .clone()
        .or(fallback_teacher)
        .filter(|target| !target.is_empty());

    let Some(target_id) = target_id else {
        send_json(
            sender,
            &SignalEnvelope {
                event: "error".to_string(),
                source: None,
                target: None,
                nickname: None,
                payload: None,
                message: Some("找不到可用目標端".to_string()),
            },
        );
        return;
    };

    envelope.source = source_id;
    envelope.target = Some(target_id.clone());

    let Ok(raw) = serde_json::to_string(&envelope) else {
        return;
    };

    let guard = hub.lock().await;
    let sent = guard
        .student_channels
        .get(&target_id)
        .map(|tx| tx.send(raw.clone()).is_ok())
        .unwrap_or(false)
        || guard
            .teacher_channels
            .get(&target_id)
            .map(|tx| tx.send(raw).is_ok())
            .unwrap_or(false);

    if !sent {
        send_json(
            sender,
            &SignalEnvelope {
                event: "error".to_string(),
                source: None,
                target: None,
                nickname: None,
                payload: None,
                message: Some("訊號傳遞失敗，目標端可能已離線".to_string()),
            },
        );
    }
}

async fn handle_socket(socket: WebSocket, hub: Arc<Mutex<SessionHub>>, role: String) {
    let (mut ws_writer, mut ws_reader) = socket.split();
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<String>();

    let writer_task = tauri::async_runtime::spawn(async move {
        while let Some(raw) = out_rx.recv().await {
            if ws_writer.send(Message::Text(raw.into())).await.is_err() {
                break;
            }
        }
    });

    let is_teacher = role == "teacher";
    let is_console = role == "console";
    let connection_id = Uuid::new_v4().to_string();

    if is_teacher {
        let mut guard = hub.lock().await;
        guard
            .teacher_channels
            .insert(connection_id.clone(), out_tx.clone());

        let students: Vec<StudentSession> = guard.students.values().cloned().collect();
        send_json(
            &out_tx,
            &SignalEnvelope {
                event: "teacher-ready".to_string(),
                source: Some(connection_id.clone()),
                target: None,
                nickname: None,
                payload: Some(json!({ "students": students })),
                message: None,
            },
        );
    } else if is_console {
        let mut guard = hub.lock().await;
        guard
            .console_channels
            .insert(connection_id.clone(), out_tx.clone());

        let students: Vec<StudentSession> = guard.students.values().cloned().collect();
        send_json(
            &out_tx,
            &SignalEnvelope {
                event: "console-ready".to_string(),
                source: Some(connection_id.clone()),
                target: None,
                nickname: None,
                payload: Some(json!({ "students": students })),
                message: None,
            },
        );
    }

    let mut student_id: Option<String> = if is_teacher {
        None
    } else if is_console {
        None
    } else {
        Some(connection_id.clone())
    };

    while let Some(message_result) = ws_reader.next().await {
        let Ok(message) = message_result else {
            break;
        };

        let Message::Text(text) = message else {
            continue;
        };

        let Ok(mut incoming) = serde_json::from_str::<SignalEnvelope>(&text) else {
            send_json(
                &out_tx,
                &SignalEnvelope {
                    event: "error".to_string(),
                    source: None,
                    target: None,
                    nickname: None,
                    payload: None,
                    message: Some("訊息格式錯誤".to_string()),
                },
            );
            continue;
        };

        match incoming.event.as_str() {
            "join" if !is_teacher && !is_console => {
                let Some(nickname) = incoming.nickname.clone() else {
                    send_json(
                        &out_tx,
                        &SignalEnvelope {
                            event: "error".to_string(),
                            source: None,
                            target: None,
                            nickname: None,
                            payload: None,
                            message: Some("暱稱不可為空".to_string()),
                        },
                    );
                    continue;
                };

                if nickname.trim().is_empty() {
                    send_json(
                        &out_tx,
                        &SignalEnvelope {
                            event: "error".to_string(),
                            source: None,
                            target: None,
                            nickname: None,
                            payload: None,
                            message: Some("請輸入有效暱稱".to_string()),
                        },
                    );
                    continue;
                }

                let conn_id = student_id
                    .get_or_insert_with(|| Uuid::new_v4().to_string())
                    .clone();

                let teacher_target = {
                    let mut guard = hub.lock().await;
                    guard
                        .student_channels
                        .insert(conn_id.clone(), out_tx.clone());
                    guard.students.insert(
                        conn_id.clone(),
                        StudentSession {
                            connection_id: conn_id.clone(),
                            nickname: nickname.clone(),
                            connected: true,
                        },
                    );
                    guard.teacher_channels.keys().next().cloned()
                };

                send_json(
                    &out_tx,
                    &SignalEnvelope {
                        event: "joined".to_string(),
                        source: Some(conn_id),
                        target: teacher_target,
                        nickname: Some(nickname),
                        payload: None,
                        message: None,
                    },
                );

                broadcast_student_list(&hub).await;
            }
            "offer" | "answer" | "ice-candidate" => {
                if is_console {
                    continue;
                }
                let source_id = if is_teacher {
                    Some(connection_id.clone())
                } else {
                    student_id.clone()
                };
                let fallback_teacher = if is_teacher {
                    None
                } else {
                    let guard = hub.lock().await;
                    guard.teacher_channels.keys().next().cloned()
                };
                forward_signal(&hub, fallback_teacher, source_id, incoming, &out_tx).await;
            }
            "disconnect" => {
                break;
            }
            _ => {
                incoming.source = if is_teacher {
                    Some(connection_id.clone())
                } else {
                    student_id.clone()
                };
                send_json(&out_tx, &incoming);
            }
        }
    }

    if is_teacher {
        let mut guard = hub.lock().await;
        guard.teacher_channels.remove(&connection_id);
    } else if is_console {
        let mut guard = hub.lock().await;
        guard.console_channels.remove(&connection_id);
    } else if let Some(id) = student_id {
        let mut guard = hub.lock().await;
        guard.students.remove(&id);
        guard.student_channels.remove(&id);
        drop(guard);
        broadcast_student_list(&hub).await;
    }

    writer_task.abort();
}

async fn start_server_impl(runtime: Arc<Mutex<BackendRuntime>>) -> Result<ServerInfo, String> {
    let (port, hub) = {
        let mut guard = runtime.lock().await;
        if guard.running.is_some() {
            return Ok(guard.control.to_info());
        }
        guard.control.status = ServiceStatus::Starting;
        guard.control.error = None;
        guard.control.refresh_ip_url();
        (guard.control.port, guard.hub.clone())
    };

    let listener = match TcpListener::bind(("0.0.0.0", port)).await {
        Ok(listener) => listener,
        Err(error) => {
            let mut guard = runtime.lock().await;
            guard.control.status = ServiceStatus::Error;
            guard.control.error = Some(error.to_string());
            return Err(format!("伺服器啟動失敗: {error}"));
        }
    };

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let app_assets =
        ServeDir::new("../dist").not_found_service(ServeFile::new("../dist/index.html"));
    let root_assets = ServeDir::new("../dist/assets");
    let router = Router::new()
        .route("/", get(student_page))
        .route("/student", get(student_page))
        .route("/teacher", get(teacher_page))
        .route("/health", get(health))
        .route_service("/vite.svg", ServeFile::new("../dist/vite.svg"))
        .route("/ws", get(ws_handler))
        .nest_service("/assets", root_assets)
        .nest_service("/app", app_assets)
        .with_state(HttpState { hub });

    let join_handle = tauri::async_runtime::spawn(async move {
        let _ = axum::serve(listener, router)
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            })
            .await;
    });

    let info = {
        let mut guard = runtime.lock().await;
        guard.running = Some(ServerHandle {
            shutdown: shutdown_tx,
            join_handle,
        });
        guard.control.status = ServiceStatus::Running;
        guard.control.error = None;
        guard.control.to_info()
    };

    Ok(info)
}

async fn stop_server_impl(runtime: Arc<Mutex<BackendRuntime>>) -> Result<ServerInfo, String> {
    let handle = {
        let mut guard = runtime.lock().await;
        guard.running.take()
    };

    if let Some(handle) = handle {
        let _ = handle.shutdown.send(());
        let _ = handle.join_handle.await;
    }

    let info = {
        let mut guard = runtime.lock().await;
        guard.control.status = ServiceStatus::Stopped;
        guard.control.error = None;
        guard.hub = Arc::new(Mutex::new(SessionHub::default()));
        guard.control.to_info()
    };

    Ok(info)
}

#[tauri::command]
async fn start_server(state: tauri::State<'_, BackendState>) -> Result<ServerInfo, String> {
    start_server_impl(state.inner.clone()).await
}

#[tauri::command]
async fn stop_server(state: tauri::State<'_, BackendState>) -> Result<ServerInfo, String> {
    stop_server_impl(state.inner.clone()).await
}

#[tauri::command]
async fn get_server_info(state: tauri::State<'_, BackendState>) -> Result<ServerInfo, String> {
    let mut guard = state.inner.lock().await;
    guard.control.refresh_ip_url();
    Ok(guard.control.to_info())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(BackendState::new())
        .setup(|app| {
            let runtime = app.state::<BackendState>().inner.clone();
            tauri::async_runtime::spawn(async move {
                let _ = start_server_impl(runtime).await;
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            start_server,
            stop_server,
            get_server_info
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
