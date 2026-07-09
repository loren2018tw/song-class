use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::http::header::{ACCESS_CONTROL_ALLOW_ORIGIN, HOST};
use axum::http::StatusCode;
use axum::http::{HeaderMap, HeaderValue};
use axum::response::{IntoResponse, Redirect};
use axum::routing::{delete, get, patch, post};
use axum::{Json, Router};
use chrono::{Datelike, Local};
use futures_util::{SinkExt, StreamExt};
use local_ip_address::local_ip;
use rusqlite::{params, params_from_iter, types::Value as SqlValue, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::menu::{Menu, MenuBuilder, MenuItem, SubmenuBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Manager};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot, Mutex};
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use uuid::Uuid;

const DEFAULT_PORT: u16 = 17860;
const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
include!(concat!(env!("OUT_DIR"), "/generated_migrations.rs"));

fn latest_schema_version() -> i32 {
    GENERATED_MIGRATIONS
        .last()
        .map(|(version, _)| *version)
        .unwrap_or(0)
}

fn baseline_migration() -> Result<(i32, &'static str), String> {
    GENERATED_MIGRATIONS
        .first()
        .map(|(version, sql)| (*version, *sql))
        .ok_or_else(|| "找不到任何可用的 migration 檔案".to_string())
}

fn resolve_app_version(app: &tauri::App) -> String {
    app.config()
        .version
        .clone()
        .unwrap_or_else(|| APP_VERSION.to_string())
}

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
    student_id: i64,
    classroom_id: i64,
    seat_no_text: String,
    nickname: String,
    connected: bool,
    focus_status: String,
    focus_updated_at: u64,
}

#[derive(Debug, Default)]
struct SessionHub {
    students: HashMap<String, StudentSession>,
    student_channels: HashMap<String, mpsc::UnboundedSender<String>>,
    teacher_channels: HashMap<String, mpsc::UnboundedSender<String>>,
    console_channels: HashMap<String, mpsc::UnboundedSender<String>>,
}

#[derive(Debug, Clone, Serialize)]
struct ClassroomSummary {
    id: i64,
    name: String,
    line_enabled: bool,
    #[serde(serialize_with = "mask_sensitive")]
    line_channel_access_token: String,
    #[serde(serialize_with = "mask_sensitive")]
    line_channel_secret: String,
    line_rich_menu_id: String,
}

#[derive(Debug, Clone, Serialize)]
struct ClassroomStudent {
    id: i64,
    classroom_id: i64,
    seat_no_text: String,
    nickname: String,
    display_name: String,
    occupied: bool,
    points: i64,
    group_no: i64,
}

#[derive(Debug, Clone, Serialize)]
struct ClassroomStatePayload {
    current_classroom: ClassroomSummary,
    classrooms: Vec<ClassroomSummary>,
    students: Vec<ClassroomStudent>,
}

#[derive(Clone)]
struct HttpState {
    runtime: Arc<Mutex<BackendRuntime>>,
    hub: Arc<Mutex<SessionHub>>,
    app_version: String,
}

struct BackendRuntime {
    control: ServerControl,
    hub: Arc<Mutex<SessionHub>>,
    frontend_assets_root: PathBuf,
    frontend_assets_candidates: Vec<PathBuf>,
    tauri_resource_dir: Option<String>,
    db_path: PathBuf,
    current_classroom_id: i64,
    running: Option<ServerHandle>,
}

#[derive(Clone)]
struct BackendState {
    inner: Arc<Mutex<BackendRuntime>>,
    app_version: String,
}

impl BackendState {
    fn new(
        app_version: String,
        frontend_assets_root: PathBuf,
        frontend_assets_candidates: Vec<PathBuf>,
        tauri_resource_dir: Option<String>,
        db_path: PathBuf,
        current_classroom_id: i64,
    ) -> Self {
        Self {
            app_version,
            inner: Arc::new(Mutex::new(BackendRuntime {
                control: ServerControl::new(DEFAULT_PORT),
                hub: Arc::new(Mutex::new(SessionHub::default())),
                frontend_assets_root,
                frontend_assets_candidates,
                tauri_resource_dir,
                db_path,
                current_classroom_id,
                running: None,
            })),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct ServerDebugInfo {
    frontend_assets_root: String,
    frontend_index_exists: bool,
    frontend_assets_dir_exists: bool,
    checked_frontend_paths: Vec<String>,
    executable_path: Option<String>,
    tauri_resource_dir: Option<String>,
    app_teacher_url: String,
    app_student_url: String,
    teacher_redirect_url: String,
    student_redirect_url: String,
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

#[derive(Debug, Deserialize)]
struct JoinPayload {
    classroom_id: i64,
    seat_no_text: String,
}

#[derive(Debug, Deserialize)]
struct SelectClassroomRequest {
    classroom_id: i64,
}

#[derive(Debug, Deserialize)]
struct UpdateClassroomRequest {
    name: String,
    #[serde(default)]
    line_enabled: Option<bool>,
    #[serde(default)]
    line_channel_access_token: Option<String>,
    #[serde(default)]
    line_channel_secret: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UpdateStudentRequest {
    seat_no_text: String,
    nickname: String,
}

#[derive(Debug, Deserialize)]
struct UpdateStudentPointsRequest {
    student_id: i64,
    delta: i64,
}

#[derive(Debug, Deserialize)]
struct UpdateMultipleStudentPointsRequest {
    student_ids: Vec<i64>,
    delta: i64,
}

#[derive(Debug, Deserialize)]
struct UpdateAllStudentPointsRequest {
    delta: i64,
}

#[derive(Debug, Deserialize)]
struct UpdateStudentGroupRequest {
    student_id: i64,
    group_no: i64,
}

#[derive(Debug, Deserialize)]
struct UpdateGroupPointsRequest {
    group_no: i64,
    delta: i64,
}

#[derive(Debug, Deserialize)]
struct CreateClassroomRequest {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SaveClassMembersRequest {
    students: Vec<SaveClassMemberItem>,
}

#[derive(Debug, Deserialize)]
struct SaveClassMemberItem {
    id: Option<i64>,
    seat_no_text: String,
    nickname: String,
}

#[derive(Debug, Clone, Serialize)]
struct ContactBookTask {
    id: i64,
    classroom_id: i64,
    task_date: String,
    title: String,
    show_in_contact_book: bool,
    requires_tracking: bool,
    is_completed: bool,
    student_count: i64,
    submitted_count: i64,
}

#[derive(Debug, Clone, Serialize)]
struct TaskSubmissionStatus {
    student_id: i64,
    classroom_id: i64,
    seat_no_text: String,
    nickname: String,
    display_name: String,
    submitted: bool,
}

#[derive(Debug, Clone, Serialize)]
struct TaskSubmissionsPayload {
    task: ContactBookTask,
    submissions: Vec<TaskSubmissionStatus>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum TaskTab {
    ContactBook,
    Submission,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum TaskCompletionFilter {
    All,
    Unfinished,
    Completed,
}

#[derive(Debug, Deserialize)]
struct ListTasksQuery {
    date: Option<String>,
    tab: Option<TaskTab>,
    completion: Option<TaskCompletionFilter>,
    show_all_unfinished: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct CreateTaskRequest {
    task_date: String,
    title: String,
    show_in_contact_book: bool,
    requires_tracking: bool,
}

#[derive(Debug, Deserialize)]
struct UpdateTaskRequest {
    task_date: String,
    title: String,
    show_in_contact_book: bool,
    requires_tracking: bool,
}

#[derive(Debug, Deserialize)]
struct UpdateTaskSubmissionRequest {
    submitted: bool,
}

#[derive(Debug, Deserialize)]
struct SetTaskCompletionRequest {
    completed: bool,
}

#[derive(Deserialize)]
struct WsQuery {
    role: Option<String>,
}

async fn health() -> impl IntoResponse {
    Json(json!({ "ok": true }))
}

async fn app_version(state: State<HttpState>) -> impl IntoResponse {
    (
        [(ACCESS_CONTROL_ALLOW_ORIGIN, "*")],
        Json(json!({ "version": state.app_version.clone() })),
    )
}

fn redirect_to_app_mode(mode: &str, base: Option<&str>) -> Redirect {
    let target = if let Some(base_url) = base.filter(|value| !value.trim().is_empty()) {
        format!("/app?mode={mode}&base={base_url}")
    } else {
        format!("/app?mode={mode}")
    };

    Redirect::temporary(&target)
}

async fn student_page(Query(query): Query<HashMap<String, String>>) -> impl IntoResponse {
    redirect_to_app_mode("student", query.get("base").map(String::as_str))
}

async fn teacher_page(Query(query): Query<HashMap<String, String>>) -> impl IntoResponse {
    redirect_to_app_mode("teacher", query.get("base").map(String::as_str))
}

fn resolve_vite_host_from_header(host_header: &str) -> String {
    let trimmed = host_header.trim();
    if trimmed.is_empty() {
        return "127.0.0.1".to_string();
    }

    if trimmed.starts_with('[') {
        if let Some(end_index) = trimmed.find(']') {
            return trimmed[..=end_index].to_string();
        }
        return trimmed.to_string();
    }

    if let Some((host, _port)) = trimmed.rsplit_once(':') {
        if !host.contains(':') && !host.is_empty() {
            return host.to_string();
        }
    }

    trimmed.to_string()
}

async fn app_page(
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let mode = query.get("mode").map(String::as_str).unwrap_or("student");
    let host = headers
        .get(HOST)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .unwrap_or("127.0.0.1:17860");
    let vite_host = resolve_vite_host_from_header(host);

    let target = format!("http://{vite_host}:1420/?mode={mode}&base=http://{host}");
    Redirect::temporary(&target)
}

async fn ws_handler(
    Query(query): Query<WsQuery>,
    ws: WebSocketUpgrade,
    State(state): State<HttpState>,
) -> impl IntoResponse {
    let role = query.role.unwrap_or_else(|| "student".to_string());
    ws.on_upgrade(move |socket| handle_socket(socket, state.runtime, state.hub, role))
}

fn resolve_local_ip() -> String {
    local_ip()
        .map(|ip| ip.to_string())
        .unwrap_or_else(|_| "127.0.0.1".to_string())
}

fn format_host_for_url(host: &str) -> String {
    if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]")
    } else {
        host.to_string()
    }
}

fn send_json(tx: &mpsc::UnboundedSender<String>, envelope: &SignalEnvelope) {
    if let Ok(payload) = serde_json::to_string(envelope) {
        let _ = tx.send(payload);
    }
}

fn send_ws_error(tx: &mpsc::UnboundedSender<String>, message: impl Into<String>) {
    let message = message.into();
    eprintln!("[ws-error] {message}");
    send_json(
        tx,
        &SignalEnvelope {
            event: "error".to_string(),
            source: None,
            target: None,
            nickname: None,
            payload: None,
            message: Some(message),
        },
    );
}

fn seat_nickname_display(seat_no_text: &str, nickname: &str) -> String {
    let trimmed = nickname.trim();
    if trimmed.is_empty() {
        seat_no_text.to_string()
    } else {
        format!("{seat_no_text}{trimmed}")
    }
}

fn normalize_seat_no_text(raw: &str) -> String {
    raw.trim().to_string()
}

fn read_user_version(conn: &Connection) -> Result<i32, String> {
    conn.query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|error| format!("讀取資料庫版本失敗: {error}"))
}

fn write_user_version(conn: &Connection, version: i32) -> Result<(), String> {
    conn.pragma_update(None, "user_version", version)
        .map_err(|error| format!("寫入資料庫版本失敗: {error}"))
}

fn has_legacy_v0_schema(conn: &Connection) -> Result<bool, String> {
    let sql = "
        SELECT COUNT(1)
        FROM sqlite_master
        WHERE type = 'table'
          AND name IN ('classrooms', 'students', 'tasks', 'task_submissions')
    ";
    let table_count: i64 = conn
        .query_row(sql, [], |row| row.get(0))
        .map_err(|error| format!("檢查資料表結構失敗: {error}"))?;
    Ok(table_count > 0)
}

fn apply_pending_migrations(conn: &Connection, current_version: i32) -> Result<(), String> {
    let latest_version = latest_schema_version();
    if current_version > latest_version {
        return Err(format!(
            "資料庫版本 ({current_version}) 高於目前程式支援版本 ({latest_version})"
        ));
    }

    for (version, sql) in GENERATED_MIGRATIONS {
        if *version > current_version {
            conn.execute_batch(sql)
                .map_err(|error| format!("套用 migration v{version} 失敗: {error}"))?;
            write_user_version(conn, *version)?;
        }
    }

    Ok(())
}

fn init_database(db_path: &PathBuf) -> Result<i64, String> {
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("建立資料庫目錄失敗: {error}"))?;
    }

    let is_new_database = !db_path.exists();
    let mut conn = Connection::open(db_path).map_err(|error| format!("開啟資料庫失敗: {error}"))?;
    let (baseline_version, baseline_sql) = baseline_migration()?;

    if is_new_database {
        conn.execute_batch(baseline_sql)
            .map_err(|error| format!("套用 DDL 失敗: {error}"))?;
        write_user_version(&conn, baseline_version)?;
    } else {
        let user_version = read_user_version(&conn)?;
        if user_version == 0 {
            if has_legacy_v0_schema(&conn)? {
                write_user_version(&conn, baseline_version)?;
            } else {
                conn.execute_batch(baseline_sql)
                    .map_err(|error| format!("套用 DDL 失敗: {error}"))?;
                write_user_version(&conn, baseline_version)?;
            }
        }
    }

    let current_version = read_user_version(&conn)?;
    apply_pending_migrations(&conn, current_version)?;

    let classroom_count: i64 = conn
        .query_row("SELECT COUNT(1) FROM classrooms", [], |row| row.get(0))
        .map_err(|error| format!("讀取班級數量失敗: {error}"))?;

    if classroom_count == 0 {
        conn.execute(
            "INSERT INTO classrooms (name) VALUES (?1)",
            params!["預設班級"],
        )
        .map_err(|error| format!("建立首個班級失敗: {error}"))?;

        let classroom_id = conn.last_insert_rowid();
        let tx = conn
            .transaction()
            .map_err(|error| format!("建立預設學生交易失敗: {error}"))?;
        for seat in 1..=30 {
            let seat_no_text = format!("{seat:02}");
            tx.execute(
                "INSERT INTO students (classroom_id, seat_no_text, nickname) VALUES (?1, ?2, ?3)",
                params![classroom_id, seat_no_text, ""],
            )
            .map_err(|error| format!("建立預設學生失敗: {error}"))?;
        }
        tx.commit()
            .map_err(|error| format!("提交預設學生失敗: {error}"))?;
    }

    conn.query_row(
        "SELECT id FROM classrooms ORDER BY id ASC LIMIT 1",
        [],
        |row| row.get(0),
    )
    .map_err(|error| format!("讀取目前班級失敗: {error}"))
}

fn load_classrooms(conn: &Connection) -> Result<Vec<ClassroomSummary>, String> {
    let mut statement = conn
        .prepare(
            "SELECT id, name, line_enabled, line_channel_access_token, \
             line_channel_secret, line_rich_menu_id \
             FROM classrooms ORDER BY id ASC",
        )
        .map_err(|error| format!("準備班級查詢失敗: {error}"))?;

    let rows = statement
        .query_map([], |row| {
            Ok(ClassroomSummary {
                id: row.get(0)?,
                name: row.get(1)?,
                line_enabled: row.get::<_, i64>(2)? != 0,
                line_channel_access_token: row.get(3)?,
                line_channel_secret: row.get(4)?,
                line_rich_menu_id: row.get(5)?,
            })
        })
        .map_err(|error| format!("查詢班級清單失敗: {error}"))?;

    let mut classrooms = Vec::new();
    for item in rows {
        classrooms.push(item.map_err(|error| format!("讀取班級資料失敗: {error}"))?);
    }
    Ok(classrooms)
}

fn load_students_by_classroom(
    conn: &Connection,
    classroom_id: i64,
    occupied_seats: &HashSet<String>,
) -> Result<Vec<ClassroomStudent>, String> {
    let mut statement = conn
        .prepare(
            "SELECT id,
                    classroom_id,
                    seat_no_text,
                    nickname,
                    COALESCE(points, 0),
                    COALESCE(group_no, 0)
             FROM students
             WHERE classroom_id = ?1
             ORDER BY seat_no_text ASC",
        )
        .map_err(|error| format!("準備學生查詢失敗: {error}"))?;

    let rows = statement
        .query_map(params![classroom_id], |row| {
            let seat_no_text: String = row.get(2)?;
            let nickname: String = row.get(3)?;
            Ok(ClassroomStudent {
                id: row.get(0)?,
                classroom_id: row.get(1)?,
                display_name: seat_nickname_display(&seat_no_text, &nickname),
                occupied: occupied_seats.contains(&seat_no_text),
                seat_no_text,
                nickname,
                points: row.get(4)?,
                group_no: row.get(5)?,
            })
        })
        .map_err(|error| format!("查詢學生清單失敗: {error}"))?;

    let mut students = Vec::new();
    for item in rows {
        students.push(item.map_err(|error| format!("讀取學生資料失敗: {error}"))?);
    }
    Ok(students)
}

fn to_db_bool(value: bool) -> i64 {
    if value {
        1
    } else {
        0
    }
}

fn from_db_bool(value: i64) -> bool {
    value != 0
}

fn normalize_task_date(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.len() != 10 {
        return None;
    }

    let bytes = trimmed.as_bytes();
    if bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }

    let year_ok = bytes[0..4].iter().all(|b| b.is_ascii_digit());
    let month_ok = bytes[5..7].iter().all(|b| b.is_ascii_digit());
    let day_ok = bytes[8..10].iter().all(|b| b.is_ascii_digit());
    if !year_ok || !month_ok || !day_ok {
        return None;
    }

    Some(trimmed.to_string())
}

fn validate_task_flags(show_in_contact_book: bool, requires_tracking: bool) -> Result<(), String> {
    if !show_in_contact_book && !requires_tracking {
        return Err("至少要勾選顯示在聯絡簿或需控管其中一項".to_string());
    }
    Ok(())
}

fn load_task_for_classroom(
    conn: &Connection,
    classroom_id: i64,
    task_id: i64,
) -> Result<Option<ContactBookTask>, String> {
    conn.query_row(
        "SELECT t.id,
                t.classroom_id,
                t.task_date,
                t.title,
                t.show_in_contact_book,
                t.requires_tracking,
                t.is_completed,
                (SELECT COUNT(1)
                   FROM students s
                  WHERE s.classroom_id = t.classroom_id) AS student_count,
                (SELECT COUNT(1)
                   FROM task_submissions ts
                   JOIN students s ON s.id = ts.student_id
                  WHERE ts.task_id = t.id
                    AND ts.submitted = 1
                    AND s.classroom_id = t.classroom_id) AS submitted_count
           FROM tasks t
          WHERE t.id = ?1 AND t.classroom_id = ?2",
        params![task_id, classroom_id],
        |row| {
            Ok(ContactBookTask {
                id: row.get(0)?,
                classroom_id: row.get(1)?,
                task_date: row.get(2)?,
                title: row.get(3)?,
                show_in_contact_book: from_db_bool(row.get::<_, i64>(4)?),
                requires_tracking: from_db_bool(row.get::<_, i64>(5)?),
                is_completed: from_db_bool(row.get::<_, i64>(6)?),
                student_count: row.get(7)?,
                submitted_count: row.get(8)?,
            })
        },
    )
    .optional()
    .map_err(|error| format!("讀取任務資料失敗: {error}"))
}

fn list_tasks_for_classroom(
    conn: &Connection,
    classroom_id: i64,
    query: &ListTasksQuery,
) -> Result<Vec<ContactBookTask>, String> {
    let mut sql = String::from(
        "SELECT t.id,
                t.classroom_id,
                t.task_date,
                t.title,
                t.show_in_contact_book,
                t.requires_tracking,
                t.is_completed,
                (SELECT COUNT(1)
                   FROM students s
                  WHERE s.classroom_id = t.classroom_id) AS student_count,
                (SELECT COUNT(1)
                   FROM task_submissions ts
                   JOIN students s ON s.id = ts.student_id
                  WHERE ts.task_id = t.id
                    AND ts.submitted = 1
                    AND s.classroom_id = t.classroom_id) AS submitted_count
           FROM tasks t
          WHERE t.classroom_id = ?",
    );
    let mut bind_values = vec![SqlValue::Integer(classroom_id)];

    match query.tab.unwrap_or(TaskTab::ContactBook) {
        TaskTab::ContactBook => {
            sql.push_str(" AND t.show_in_contact_book = 1");
        }
        TaskTab::Submission => {
            sql.push_str(" AND t.requires_tracking = 1");
        }
    }

    if query.show_all_unfinished.unwrap_or(false) {
        sql.push_str(" AND t.is_completed = 0");
    } else {
        if let Some(date) = query.date.as_ref().and_then(|raw| normalize_task_date(raw)) {
            sql.push_str(" AND t.task_date = ?");
            bind_values.push(SqlValue::Text(date));
        }

        match query.completion.unwrap_or(TaskCompletionFilter::All) {
            TaskCompletionFilter::All => {}
            TaskCompletionFilter::Unfinished => sql.push_str(" AND t.is_completed = 0"),
            TaskCompletionFilter::Completed => sql.push_str(" AND t.is_completed = 1"),
        }
    }

    sql.push_str(" ORDER BY t.task_date ASC, t.id ASC");

    let mut statement = conn
        .prepare(&sql)
        .map_err(|error| format!("準備任務查詢失敗: {error}"))?;
    let rows = statement
        .query_map(params_from_iter(bind_values), |row| {
            Ok(ContactBookTask {
                id: row.get(0)?,
                classroom_id: row.get(1)?,
                task_date: row.get(2)?,
                title: row.get(3)?,
                show_in_contact_book: from_db_bool(row.get::<_, i64>(4)?),
                requires_tracking: from_db_bool(row.get::<_, i64>(5)?),
                is_completed: from_db_bool(row.get::<_, i64>(6)?),
                student_count: row.get(7)?,
                submitted_count: row.get(8)?,
            })
        })
        .map_err(|error| format!("查詢任務清單失敗: {error}"))?;

    let mut tasks = Vec::new();
    for row in rows {
        tasks.push(row.map_err(|error| format!("讀取任務資料失敗: {error}"))?);
    }
    Ok(tasks)
}

fn load_task_submission_statuses(
    conn: &Connection,
    classroom_id: i64,
    task_id: i64,
) -> Result<Vec<TaskSubmissionStatus>, String> {
    let mut statement = conn
        .prepare(
            "SELECT s.id,
                    s.classroom_id,
                    s.seat_no_text,
                    s.nickname,
                    COALESCE(ts.submitted, 0)
               FROM students s
               LEFT JOIN task_submissions ts
                 ON ts.student_id = s.id
                AND ts.task_id = ?2
              WHERE s.classroom_id = ?1
              ORDER BY s.seat_no_text ASC",
        )
        .map_err(|error| format!("準備繳交狀態查詢失敗: {error}"))?;

    let rows = statement
        .query_map(params![classroom_id, task_id], |row| {
            let seat_no_text: String = row.get(2)?;
            let nickname: String = row.get(3)?;
            Ok(TaskSubmissionStatus {
                student_id: row.get(0)?,
                classroom_id: row.get(1)?,
                display_name: seat_nickname_display(&seat_no_text, &nickname),
                seat_no_text,
                nickname,
                submitted: from_db_bool(row.get::<_, i64>(4)?),
            })
        })
        .map_err(|error| format!("查詢繳交狀態失敗: {error}"))?;

    let mut statuses = Vec::new();
    for row in rows {
        statuses.push(row.map_err(|error| format!("讀取繳交狀態失敗: {error}"))?);
    }
    Ok(statuses)
}

fn sync_task_completion_from_submissions(
    conn: &Connection,
    classroom_id: i64,
    task_id: i64,
) -> Result<bool, String> {
    let student_count: i64 = conn
        .query_row(
            "SELECT COUNT(1) FROM students WHERE classroom_id = ?1",
            params![classroom_id],
            |row| row.get(0),
        )
        .map_err(|error| format!("讀取學生數量失敗: {error}"))?;

    let submitted_count: i64 = conn
        .query_row(
            "SELECT COUNT(1)
               FROM task_submissions ts
               JOIN students s ON s.id = ts.student_id
              WHERE ts.task_id = ?1
                AND ts.submitted = 1
                AND s.classroom_id = ?2",
            params![task_id, classroom_id],
            |row| row.get(0),
        )
        .map_err(|error| format!("讀取繳交數量失敗: {error}"))?;

    let all_submitted = student_count > 0 && submitted_count == student_count;
    conn.execute(
        "UPDATE tasks
            SET is_completed = ?1
          WHERE id = ?2
            AND classroom_id = ?3",
        params![to_db_bool(all_submitted), task_id, classroom_id],
    )
    .map_err(|error| format!("同步任務完成狀態失敗: {error}"))?;

    Ok(all_submitted)
}

fn set_all_task_submission_states(
    conn: &Connection,
    classroom_id: i64,
    task_id: i64,
    submitted: bool,
) -> Result<(), String> {
    let mut students_stmt = conn
        .prepare("SELECT id FROM students WHERE classroom_id = ?1")
        .map_err(|error| format!("準備學生清單查詢失敗: {error}"))?;

    let student_rows = students_stmt
        .query_map(params![classroom_id], |row| row.get::<_, i64>(0))
        .map_err(|error| format!("查詢學生清單失敗: {error}"))?;

    let mut student_ids = Vec::new();
    for row in student_rows {
        student_ids.push(row.map_err(|error| format!("讀取學生資料失敗: {error}"))?);
    }

    for student_id in student_ids {
        conn.execute(
            "INSERT INTO task_submissions (task_id, student_id, submitted)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(task_id, student_id)
             DO UPDATE SET submitted = excluded.submitted",
            params![task_id, student_id, to_db_bool(submitted)],
        )
        .map_err(|error| format!("更新繳交狀態失敗: {error}"))?;
    }

    conn.execute(
        "UPDATE tasks
            SET is_completed = ?1
          WHERE id = ?2 AND classroom_id = ?3",
        params![to_db_bool(submitted), task_id, classroom_id],
    )
    .map_err(|error| format!("更新任務完成狀態失敗: {error}"))?;

    Ok(())
}

async fn build_classroom_state(
    runtime: &Arc<Mutex<BackendRuntime>>,
) -> Result<ClassroomStatePayload, String> {
    let (db_path, current_classroom_id, hub) = {
        let guard = runtime.lock().await;
        (
            guard.db_path.clone(),
            guard.current_classroom_id,
            guard.hub.clone(),
        )
    };

    let occupied_seats = {
        let guard = hub.lock().await;
        guard
            .students
            .values()
            .filter(|student| student.classroom_id == current_classroom_id)
            .map(|student| student.seat_no_text.clone())
            .collect::<HashSet<_>>()
    };

    let conn = Connection::open(db_path).map_err(|error| format!("開啟資料庫失敗: {error}"))?;
    let classrooms = load_classrooms(&conn)?;
    let current_classroom = classrooms
        .iter()
        .find(|classroom| classroom.id == current_classroom_id)
        .cloned()
        .ok_or_else(|| "找不到目前班級".to_string())?;
    let students = load_students_by_classroom(&conn, current_classroom_id, &occupied_seats)?;

    Ok(ClassroomStatePayload {
        current_classroom,
        classrooms,
        students,
    })
}

async fn broadcast_classroom_state(runtime: &Arc<Mutex<BackendRuntime>>) {
    let state = match build_classroom_state(runtime).await {
        Ok(state) => state,
        Err(error) => {
            eprintln!("[classroom-state] build failed: {error}");
            return;
        }
    };

    let raw = match serde_json::to_string(&SignalEnvelope {
        event: "classroom-state".to_string(),
        source: None,
        target: None,
        nickname: None,
        payload: Some(json!({ "state": state })),
        message: None,
    }) {
        Ok(raw) => raw,
        Err(_) => return,
    };

    let hub = {
        let guard = runtime.lock().await;
        guard.hub.clone()
    };

    let mut guard = hub.lock().await;
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

    for (id, tx) in &guard.student_channels {
        if tx.send(raw.clone()).is_err() {
            stale.push(id.clone());
        }
    }

    for id in stale {
        guard.teacher_channels.remove(&id);
        guard.console_channels.remove(&id);
        guard.student_channels.remove(&id);
    }
}

fn api_error(status: StatusCode, message: impl Into<String>) -> (StatusCode, Json<Value>) {
    let message = message.into();
    eprintln!("[api-error] {status}: {message}");
    (status, Json(json!({ "message": message })))
}

fn mask_sensitive<S: serde::Serializer>(value: &str, serializer: S) -> Result<S::Ok, S::Error> {
    let masked = if value.len() > 8 {
        format!("{}****", &value[..4])
    } else if !value.is_empty() {
        "****".to_string()
    } else {
        value.to_string()
    };
    serializer.serialize_str(&masked)
}

fn is_masked(value: &str) -> bool {
    value.contains("****")
}

async fn get_classroom_state_handler(
    State(state): State<HttpState>,
) -> Result<Json<ClassroomStatePayload>, (StatusCode, Json<Value>)> {
    let payload = build_classroom_state(&state.runtime)
        .await
        .map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, error))?;
    Ok(Json(payload))
}

async fn list_classrooms_handler(
    State(state): State<HttpState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let payload = build_classroom_state(&state.runtime)
        .await
        .map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, error))?;
    Ok(Json(json!({
        "classrooms": payload.classrooms,
        "current_classroom_id": payload.current_classroom.id
    })))
}

async fn list_contact_book_tasks_handler(
    State(state): State<HttpState>,
    Query(query): Query<ListTasksQuery>,
) -> Result<Json<Vec<ContactBookTask>>, (StatusCode, Json<Value>)> {
    let (db_path, classroom_id) = {
        let guard = state.runtime.lock().await;
        (guard.db_path.clone(), guard.current_classroom_id)
    };

    let conn = Connection::open(db_path).map_err(|error| {
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("開啟資料庫失敗: {error}"),
        )
    })?;

    let tasks = list_tasks_for_classroom(&conn, classroom_id, &query)
        .map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, error))?;
    Ok(Json(tasks))
}

async fn create_contact_book_task_handler(
    State(state): State<HttpState>,
    Json(body): Json<CreateTaskRequest>,
) -> Result<Json<ContactBookTask>, (StatusCode, Json<Value>)> {
    let title = body.title.trim();
    if title.is_empty() {
        return Err(api_error(StatusCode::BAD_REQUEST, "任務名稱不可為空"));
    }

    let task_date = normalize_task_date(&body.task_date)
        .ok_or_else(|| api_error(StatusCode::BAD_REQUEST, "日期格式必須為 YYYY-MM-DD"))?;
    validate_task_flags(body.show_in_contact_book, body.requires_tracking)
        .map_err(|error| api_error(StatusCode::BAD_REQUEST, error))?;

    let (db_path, classroom_id) = {
        let guard = state.runtime.lock().await;
        (guard.db_path.clone(), guard.current_classroom_id)
    };

    let conn = Connection::open(db_path).map_err(|error| {
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("開啟資料庫失敗: {error}"),
        )
    })?;

    conn.execute(
        "INSERT INTO tasks (
            classroom_id,
            task_date,
            title,
            show_in_contact_book,
            requires_tracking,
            is_completed
         ) VALUES (?1, ?2, ?3, ?4, ?5, 0)",
        params![
            classroom_id,
            task_date,
            title,
            to_db_bool(body.show_in_contact_book),
            to_db_bool(body.requires_tracking)
        ],
    )
    .map_err(|error| {
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("建立任務失敗: {error}"),
        )
    })?;

    let task_id = conn.last_insert_rowid();
    let task = load_task_for_classroom(&conn, classroom_id, task_id)
        .map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, error))?
        .ok_or_else(|| api_error(StatusCode::INTERNAL_SERVER_ERROR, "建立任務後讀取失敗"))?;

    Ok(Json(task))
}

async fn update_contact_book_task_handler(
    State(state): State<HttpState>,
    Path(task_id): Path<i64>,
    Json(body): Json<UpdateTaskRequest>,
) -> Result<Json<ContactBookTask>, (StatusCode, Json<Value>)> {
    let title = body.title.trim();
    if title.is_empty() {
        return Err(api_error(StatusCode::BAD_REQUEST, "任務名稱不可為空"));
    }

    let task_date = normalize_task_date(&body.task_date)
        .ok_or_else(|| api_error(StatusCode::BAD_REQUEST, "日期格式必須為 YYYY-MM-DD"))?;
    validate_task_flags(body.show_in_contact_book, body.requires_tracking)
        .map_err(|error| api_error(StatusCode::BAD_REQUEST, error))?;

    let (db_path, classroom_id) = {
        let guard = state.runtime.lock().await;
        (guard.db_path.clone(), guard.current_classroom_id)
    };

    let conn = Connection::open(db_path).map_err(|error| {
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("開啟資料庫失敗: {error}"),
        )
    })?;

    let updated = conn
        .execute(
            "UPDATE tasks
                SET task_date = ?1,
                    title = ?2,
                    show_in_contact_book = ?3,
                    requires_tracking = ?4
              WHERE id = ?5 AND classroom_id = ?6",
            params![
                task_date,
                title,
                to_db_bool(body.show_in_contact_book),
                to_db_bool(body.requires_tracking),
                task_id,
                classroom_id
            ],
        )
        .map_err(|error| {
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("更新任務失敗: {error}"),
            )
        })?;

    if updated == 0 {
        return Err(api_error(StatusCode::NOT_FOUND, "指定任務不存在"));
    }

    let task = load_task_for_classroom(&conn, classroom_id, task_id)
        .map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, error))?
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "指定任務不存在"))?;

    Ok(Json(task))
}

async fn delete_contact_book_task_handler(
    State(state): State<HttpState>,
    Path(task_id): Path<i64>,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    let (db_path, classroom_id) = {
        let guard = state.runtime.lock().await;
        (guard.db_path.clone(), guard.current_classroom_id)
    };

    let conn = Connection::open(db_path).map_err(|error| {
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("開啟資料庫失敗: {error}"),
        )
    })?;

    let deleted = conn
        .execute(
            "DELETE FROM tasks WHERE id = ?1 AND classroom_id = ?2",
            params![task_id, classroom_id],
        )
        .map_err(|error| {
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("刪除任務失敗: {error}"),
            )
        })?;

    if deleted == 0 {
        return Err(api_error(StatusCode::NOT_FOUND, "指定任務不存在"));
    }

    Ok(StatusCode::NO_CONTENT)
}

async fn list_task_submissions_handler(
    State(state): State<HttpState>,
    Path(task_id): Path<i64>,
) -> Result<Json<TaskSubmissionsPayload>, (StatusCode, Json<Value>)> {
    let (db_path, classroom_id) = {
        let guard = state.runtime.lock().await;
        (guard.db_path.clone(), guard.current_classroom_id)
    };

    let conn = Connection::open(db_path).map_err(|error| {
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("開啟資料庫失敗: {error}"),
        )
    })?;

    let task = load_task_for_classroom(&conn, classroom_id, task_id)
        .map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, error))?
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "指定任務不存在"))?;

    if !task.requires_tracking {
        return Err(api_error(StatusCode::BAD_REQUEST, "此任務未啟用繳交控管"));
    }

    let submissions = load_task_submission_statuses(&conn, classroom_id, task_id)
        .map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, error))?;

    Ok(Json(TaskSubmissionsPayload { task, submissions }))
}

async fn update_task_submission_handler(
    State(state): State<HttpState>,
    Path((task_id, student_id)): Path<(i64, i64)>,
    Json(body): Json<UpdateTaskSubmissionRequest>,
) -> Result<Json<ContactBookTask>, (StatusCode, Json<Value>)> {
    let (db_path, classroom_id) = {
        let guard = state.runtime.lock().await;
        (guard.db_path.clone(), guard.current_classroom_id)
    };

    let mut conn = Connection::open(db_path).map_err(|error| {
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("開啟資料庫失敗: {error}"),
        )
    })?;

    let task = load_task_for_classroom(&conn, classroom_id, task_id)
        .map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, error))?
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "指定任務不存在"))?;

    if !task.requires_tracking {
        return Err(api_error(StatusCode::BAD_REQUEST, "此任務未啟用繳交控管"));
    }

    let student_exists: Option<i64> = conn
        .query_row(
            "SELECT id FROM students WHERE id = ?1 AND classroom_id = ?2",
            params![student_id, classroom_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| {
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("查詢學生資料失敗: {error}"),
            )
        })?;
    if student_exists.is_none() {
        return Err(api_error(StatusCode::NOT_FOUND, "指定學生不存在"));
    }

    let tx = conn.transaction().map_err(|error| {
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("建立交易失敗: {error}"),
        )
    })?;

    tx.execute(
        "INSERT INTO task_submissions (task_id, student_id, submitted)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(task_id, student_id)
         DO UPDATE SET submitted = excluded.submitted",
        params![task_id, student_id, to_db_bool(body.submitted)],
    )
    .map_err(|error| {
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("更新繳交狀態失敗: {error}"),
        )
    })?;

    sync_task_completion_from_submissions(&tx, classroom_id, task_id)
        .map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, error))?;

    tx.commit().map_err(|error| {
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("提交繳交狀態失敗: {error}"),
        )
    })?;

    let task = load_task_for_classroom(&conn, classroom_id, task_id)
        .map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, error))?
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "指定任務不存在"))?;

    Ok(Json(task))
}

async fn get_reminder_boards_handler(
    State(state): State<HttpState>,
) -> Result<Json<Vec<ReminderBoard>>, (StatusCode, Json<Value>)> {
    let db_path = {
        let guard = state.runtime.lock().await;
        guard.db_path.clone()
    };

    let conn = Connection::open(db_path).map_err(|error| {
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("開啟資料庫失敗: {error}"),
        )
    })?;

    let mut stmt = conn
        .prepare("SELECT id, category, title, subtitle, icon FROM reminder_boards WHERE category = '自訂' ORDER BY id DESC")
        .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let boards = stmt
        .query_map([], |row| {
            Ok(ReminderBoard {
                id: Some(row.get(0)?),
                category: row.get(1)?,
                title: row.get(2)?,
                subtitle: row.get(3)?,
                icon: row.get(4)?,
            })
        })
        .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(boards))
}

async fn create_reminder_board_handler(
    State(state): State<HttpState>,
    Json(board): Json<ReminderBoard>,
) -> Result<Json<ReminderBoard>, (StatusCode, Json<Value>)> {
    let db_path = {
        let guard = state.runtime.lock().await;
        guard.db_path.clone()
    };

    let conn = Connection::open(db_path).map_err(|error| {
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("開啟資料庫失敗: {error}"),
        )
    })?;

    conn.execute(
        "INSERT INTO reminder_boards (category, title, subtitle, icon) VALUES (?1, ?2, ?3, ?4)",
        params![board.category, board.title, board.subtitle, board.icon],
    )
    .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let id = conn.last_insert_rowid();
    let mut new_board = board;
    new_board.id = Some(id);
    Ok(Json(new_board))
}

async fn update_reminder_board_handler(
    State(state): State<HttpState>,
    Path(id): Path<i64>,
    Json(board): Json<ReminderBoard>,
) -> Result<Json<()>, (StatusCode, Json<Value>)> {
    let db_path = {
        let guard = state.runtime.lock().await;
        guard.db_path.clone()
    };

    let conn = Connection::open(db_path).map_err(|error| {
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("開啟資料庫失敗: {error}"),
        )
    })?;

    conn.execute(
        "UPDATE reminder_boards SET title = ?1, subtitle = ?2, icon = ?3 WHERE id = ?4",
        params![board.title, board.subtitle, board.icon, id],
    )
    .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(()))
}

async fn delete_reminder_board_handler(
    State(state): State<HttpState>,
    Path(id): Path<i64>,
) -> Result<Json<()>, (StatusCode, Json<Value>)> {
    let db_path = {
        let guard = state.runtime.lock().await;
        guard.db_path.clone()
    };

    let conn = Connection::open(db_path).map_err(|error| {
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("開啟資料庫失敗: {error}"),
        )
    })?;

    conn.execute("DELETE FROM reminder_boards WHERE id = ?1", params![id])
        .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(()))
}

async fn set_task_completion_handler(
    State(state): State<HttpState>,
    Path(task_id): Path<i64>,
    Json(body): Json<SetTaskCompletionRequest>,
) -> Result<Json<ContactBookTask>, (StatusCode, Json<Value>)> {
    let (db_path, classroom_id) = {
        let guard = state.runtime.lock().await;
        (guard.db_path.clone(), guard.current_classroom_id)
    };

    let mut conn = Connection::open(db_path).map_err(|error| {
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("開啟資料庫失敗: {error}"),
        )
    })?;

    let task = load_task_for_classroom(&conn, classroom_id, task_id)
        .map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, error))?
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "指定任務不存在"))?;

    if !task.requires_tracking {
        return Err(api_error(StatusCode::BAD_REQUEST, "此任務未啟用繳交控管"));
    }

    let tx = conn.transaction().map_err(|error| {
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("建立交易失敗: {error}"),
        )
    })?;

    set_all_task_submission_states(&tx, classroom_id, task_id, body.completed)
        .map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, error))?;

    tx.commit().map_err(|error| {
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("提交任務完成狀態失敗: {error}"),
        )
    })?;

    let task = load_task_for_classroom(&conn, classroom_id, task_id)
        .map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, error))?
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "指定任務不存在"))?;

    Ok(Json(task))
}

async fn force_logout_classroom_students(
    hub: &Arc<Mutex<SessionHub>>,
    classroom_id: i64,
    reason: &str,
    message: &str,
) {
    let forced_out_ids = {
        let guard = hub.lock().await;
        let ids = guard
            .students
            .values()
            .filter(|student| student.classroom_id == classroom_id)
            .map(|student| student.connection_id.clone())
            .collect::<Vec<_>>();

        for id in &ids {
            if let Some(tx) = guard.student_channels.get(id) {
                eprintln!(
                    "[force-logout] connection_id={id} classroom_id={classroom_id} reason={reason}"
                );
                send_json(
                    tx,
                    &SignalEnvelope {
                        event: "force-logout".to_string(),
                        source: None,
                        target: Some(id.clone()),
                        nickname: None,
                        payload: Some(json!({ "reason": reason })),
                        message: Some(message.to_string()),
                    },
                );
            }
        }
        ids
    };

    if !forced_out_ids.is_empty() {
        let mut guard = hub.lock().await;
        for id in forced_out_ids {
            guard.students.remove(&id);
            guard.student_channels.remove(&id);
        }
    }
}

async fn select_classroom_handler(
    State(state): State<HttpState>,
    Json(body): Json<SelectClassroomRequest>,
) -> Result<Json<ClassroomStatePayload>, (StatusCode, Json<Value>)> {
    let (db_path, previous_classroom_id, runtime) = {
        let guard = state.runtime.lock().await;
        (
            guard.db_path.clone(),
            guard.current_classroom_id,
            state.runtime.clone(),
        )
    };

    let conn = Connection::open(db_path).map_err(|error| {
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("開啟資料庫失敗: {error}"),
        )
    })?;
    let exists: Option<i64> = conn
        .query_row(
            "SELECT id FROM classrooms WHERE id = ?1",
            params![body.classroom_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| {
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("讀取班級失敗: {error}"),
            )
        })?;
    if exists.is_none() {
        return Err(api_error(StatusCode::NOT_FOUND, "指定班級不存在"));
    }

    if previous_classroom_id != body.classroom_id {
        let hub = {
            let mut guard = runtime.lock().await;
            guard.current_classroom_id = body.classroom_id;
            guard.hub.clone()
        };

        force_logout_classroom_students(
            &hub,
            previous_classroom_id,
            "classroom-switched",
            "班級已切換，請重新選擇座號加入",
        )
        .await;

        broadcast_student_list(&hub).await;
    }

    broadcast_classroom_state(&runtime).await;
    let payload = build_classroom_state(&runtime)
        .await
        .map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, error))?;
    Ok(Json(payload))
}

async fn update_classroom_handler(
    State(state): State<HttpState>,
    Path(classroom_id): Path<i64>,
    Json(body): Json<UpdateClassroomRequest>,
) -> Result<Json<ClassroomStatePayload>, (StatusCode, Json<Value>)> {
    let class_name = body.name.trim();
    if class_name.is_empty() {
        return Err(api_error(StatusCode::BAD_REQUEST, "班級名稱不可為空"));
    }

    let runtime = state.runtime.clone();
    let db_path = {
        let guard = runtime.lock().await;
        guard.db_path.clone()
    };
    let conn = Connection::open(db_path).map_err(|error| {
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("開啟資料庫失敗: {error}"),
        )
    })?;

    let updated = conn
        .execute(
            "UPDATE classrooms SET name = ?1 WHERE id = ?2",
            params![class_name, classroom_id],
        )
        .map_err(|error| {
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("更新班級名稱失敗: {error}"),
            )
        })?;
    if updated == 0 {
        return Err(api_error(StatusCode::NOT_FOUND, "指定班級不存在"));
    }

    if let Some(enabled) = body.line_enabled {
        conn.execute(
            "UPDATE classrooms SET line_enabled = ?1 WHERE id = ?2",
            params![enabled as i64, classroom_id],
        )
        .map_err(|error| {
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("更新 LINE 啟用設定失敗: {error}"),
            )
        })?;
    }

    if let Some(token) = body.line_channel_access_token {
        if !token.is_empty() && !is_masked(&token) {
            conn.execute(
                "UPDATE classrooms SET line_channel_access_token = ?1 WHERE id = ?2",
                params![token, classroom_id],
            )
            .map_err(|error| {
                api_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("更新 LINE token 失敗: {error}"),
                )
            })?;
        }
    }

    if let Some(secret) = body.line_channel_secret {
        if !secret.is_empty() && !is_masked(&secret) {
            conn.execute(
                "UPDATE classrooms SET line_channel_secret = ?1 WHERE id = ?2",
                params![secret, classroom_id],
            )
            .map_err(|error| {
                api_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("更新 LINE secret 失敗: {error}"),
                )
            })?;
        }
    }

    broadcast_classroom_state(&runtime).await;
    let payload = build_classroom_state(&runtime)
        .await
        .map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, error))?;
    Ok(Json(payload))
}

async fn create_classroom_handler(
    State(state): State<HttpState>,
    Json(body): Json<CreateClassroomRequest>,
) -> Result<Json<ClassroomStatePayload>, (StatusCode, Json<Value>)> {
    let runtime = state.runtime.clone();
    let db_path = {
        let guard = runtime.lock().await;
        guard.db_path.clone()
    };
    let conn = Connection::open(db_path).map_err(|error| {
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("開啟資料庫失敗: {error}"),
        )
    })?;

    let suggested_name = format!(
        "新班級{}",
        conn.query_row("SELECT COUNT(1) FROM classrooms", [], |row| row
            .get::<_, i64>(0))
            .map_err(|error| {
                api_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("讀取班級數量失敗: {error}"),
                )
            })?
            + 1
    );
    let class_name = body.name.unwrap_or(suggested_name).trim().to_string();
    if class_name.is_empty() {
        return Err(api_error(StatusCode::BAD_REQUEST, "班級名稱不可為空"));
    }

    conn.execute(
        "INSERT INTO classrooms (name) VALUES (?1)",
        params![class_name],
    )
    .map_err(|error| {
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("建立班級失敗: {error}"),
        )
    })?;

    broadcast_classroom_state(&runtime).await;
    let payload = build_classroom_state(&runtime)
        .await
        .map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, error))?;
    Ok(Json(payload))
}

async fn delete_classroom_handler(
    State(state): State<HttpState>,
    Path(classroom_id): Path<i64>,
) -> Result<Json<ClassroomStatePayload>, (StatusCode, Json<Value>)> {
    let runtime = state.runtime.clone();
    let (db_path, current_classroom_id, hub) = {
        let guard = runtime.lock().await;
        (
            guard.db_path.clone(),
            guard.current_classroom_id,
            guard.hub.clone(),
        )
    };
    let conn = Connection::open(db_path).map_err(|error| {
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("開啟資料庫失敗: {error}"),
        )
    })?;

    let class_count: i64 = conn
        .query_row("SELECT COUNT(1) FROM classrooms", [], |row| row.get(0))
        .map_err(|error| {
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("讀取班級數量失敗: {error}"),
            )
        })?;
    if class_count <= 1 {
        return Err(api_error(StatusCode::BAD_REQUEST, "無法刪除唯一的班級"));
    }

    let exists: Option<i64> = conn
        .query_row(
            "SELECT id FROM classrooms WHERE id = ?1",
            params![classroom_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| {
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("讀取班級失敗: {error}"),
            )
        })?;
    if exists.is_none() {
        return Err(api_error(StatusCode::NOT_FOUND, "指定班級不存在"));
    }

    let next_classroom_id: i64 = conn
        .query_row(
            "SELECT id FROM classrooms WHERE id != ?1 ORDER BY id ASC LIMIT 1",
            params![classroom_id],
            |row| row.get(0),
        )
        .map_err(|error| {
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("讀取替代班級失敗: {error}"),
            )
        })?;

    conn.execute(
        "DELETE FROM classrooms WHERE id = ?1",
        params![classroom_id],
    )
    .map_err(|error| {
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("刪除班級失敗: {error}"),
        )
    })?;

    if current_classroom_id == classroom_id {
        let mut guard = runtime.lock().await;
        guard.current_classroom_id = next_classroom_id;
    }

    force_logout_classroom_students(
        &hub,
        classroom_id,
        "classroom-deleted",
        "班級已刪除，請重新選擇座號加入",
    )
    .await;
    broadcast_student_list(&hub).await;

    broadcast_classroom_state(&runtime).await;
    let payload = build_classroom_state(&runtime)
        .await
        .map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, error))?;
    Ok(Json(payload))
}

async fn save_class_members_handler(
    State(state): State<HttpState>,
    Path(classroom_id): Path<i64>,
    Json(body): Json<SaveClassMembersRequest>,
) -> Result<Json<ClassroomStatePayload>, (StatusCode, Json<Value>)> {
    let mut normalized = Vec::with_capacity(body.students.len());
    let mut seat_set = HashSet::new();
    for row in body.students {
        let seat_no_text = normalize_seat_no_text(&row.seat_no_text);
        if seat_no_text.is_empty() {
            return Err(api_error(StatusCode::BAD_REQUEST, "座號不可為空"));
        }
        if !seat_set.insert(seat_no_text.clone()) {
            return Err(api_error(StatusCode::CONFLICT, "座號不可重複"));
        }
        normalized.push((row.id, seat_no_text, row.nickname));
    }

    let runtime = state.runtime.clone();
    let (db_path, hub) = {
        let guard = runtime.lock().await;
        (guard.db_path.clone(), guard.hub.clone())
    };
    let active_students = {
        let mut conn = Connection::open(db_path).map_err(|error| {
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("開啟資料庫失敗: {error}"),
            )
        })?;

        let class_exists: Option<i64> = conn
            .query_row(
                "SELECT id FROM classrooms WHERE id = ?1",
                params![classroom_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| {
                api_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("讀取班級失敗: {error}"),
                )
            })?;
        if class_exists.is_none() {
            return Err(api_error(StatusCode::NOT_FOUND, "指定班級不存在"));
        }

        let mut existing_ids = HashSet::new();
        {
            let mut stmt = conn
                .prepare("SELECT id FROM students WHERE classroom_id = ?1")
                .map_err(|error| {
                    api_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("準備學生查詢失敗: {error}"),
                    )
                })?;
            let rows = stmt
                .query_map(params![classroom_id], |row| row.get::<_, i64>(0))
                .map_err(|error| {
                    api_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("查詢學生清單失敗: {error}"),
                    )
                })?;
            for row in rows {
                existing_ids.insert(row.map_err(|error| {
                    api_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("讀取學生資料失敗: {error}"),
                    )
                })?);
            }
        }

        let keep_ids = normalized
            .iter()
            .filter_map(|(id, _, _)| *id)
            .collect::<HashSet<_>>();
        for id in &keep_ids {
            if !existing_ids.contains(id) {
                return Err(api_error(StatusCode::NOT_FOUND, "指定學生不存在"));
            }
        }

        {
            let tx = conn.transaction().map_err(|error| {
                api_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("建立交易失敗: {error}"),
                )
            })?;

            for id in existing_ids.difference(&keep_ids) {
                tx.execute(
                    "DELETE FROM students WHERE id = ?1 AND classroom_id = ?2",
                    params![id, classroom_id],
                )
                .map_err(|error| {
                    api_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("刪除學生失敗: {error}"),
                    )
                })?;
            }

            for (id, seat_no_text, nickname) in &normalized {
                let result = if let Some(student_id) = id {
                    tx.execute(
                        "UPDATE students
                         SET seat_no_text = ?1,
                             nickname = ?2
                         WHERE id = ?3 AND classroom_id = ?4",
                        params![seat_no_text, nickname, student_id, classroom_id],
                    )
                } else {
                    tx.execute(
                        "INSERT INTO students (classroom_id, seat_no_text, nickname) VALUES (?1, ?2, ?3)",
                        params![classroom_id, seat_no_text, nickname],
                    )
                };

                if let Err(error) = result {
                    let message = error.to_string();
                    if message.contains("UNIQUE") {
                        return Err(api_error(StatusCode::CONFLICT, "座號不可重複"));
                    }
                    return Err(api_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("儲存學生資料失敗: {error}"),
                    ));
                }
            }

            tx.commit().map_err(|error| {
                api_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("提交學生資料失敗: {error}"),
                )
            })?;
        }

        let mut active_students = HashMap::new();
        {
            let mut stmt = conn
                .prepare("SELECT id, seat_no_text, nickname FROM students WHERE classroom_id = ?1")
                .map_err(|error| {
                    api_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("準備學生查詢失敗: {error}"),
                    )
                })?;
            let rows = stmt
                .query_map(params![classroom_id], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })
                .map_err(|error| {
                    api_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("查詢學生清單失敗: {error}"),
                    )
                })?;
            for row in rows {
                let (id, seat_no_text, nickname) = row.map_err(|error| {
                    api_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("讀取學生資料失敗: {error}"),
                    )
                })?;
                active_students.insert(id, (seat_no_text, nickname));
            }
        }

        active_students
    };

    let mut stale_connections = Vec::new();
    {
        let mut guard = hub.lock().await;
        for session in guard.students.values_mut() {
            if session.classroom_id != classroom_id {
                continue;
            }

            if let Some((seat_no_text, nickname)) = active_students.get(&session.student_id) {
                session.seat_no_text = seat_no_text.clone();
                session.nickname = seat_nickname_display(seat_no_text, nickname);
            } else {
                stale_connections.push(session.connection_id.clone());
            }
        }

        for connection_id in &stale_connections {
            if let Some(tx) = guard.student_channels.get(connection_id) {
                send_json(
                    tx,
                    &SignalEnvelope {
                        event: "force-logout".to_string(),
                        source: None,
                        target: Some(connection_id.clone()),
                        nickname: None,
                        payload: Some(json!({ "reason": "class-members-updated" })),
                        message: Some("班級名單已更新，請重新選擇座號加入".to_string()),
                    },
                );
            }
        }

        for connection_id in stale_connections {
            guard.students.remove(&connection_id);
            guard.student_channels.remove(&connection_id);
        }
    }

    broadcast_student_list(&hub).await;
    broadcast_classroom_state(&runtime).await;
    let payload = build_classroom_state(&runtime)
        .await
        .map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, error))?;
    Ok(Json(payload))
}

async fn update_student_handler(
    State(state): State<HttpState>,
    Path((classroom_id, student_id)): Path<(i64, i64)>,
    Json(body): Json<UpdateStudentRequest>,
) -> Result<Json<ClassroomStatePayload>, (StatusCode, Json<Value>)> {
    let seat_no_text = normalize_seat_no_text(&body.seat_no_text);
    if seat_no_text.is_empty() {
        return Err(api_error(StatusCode::BAD_REQUEST, "座號不可為空"));
    }

    let runtime = state.runtime.clone();
    let db_path = {
        let guard = runtime.lock().await;
        guard.db_path.clone()
    };
    let conn = Connection::open(db_path).map_err(|error| {
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("開啟資料庫失敗: {error}"),
        )
    })?;

    let duplicate: Option<i64> = conn
        .query_row(
            "SELECT id FROM students WHERE classroom_id = ?1 AND seat_no_text = ?2 AND id != ?3",
            params![classroom_id, seat_no_text, student_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| {
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("檢查座號重複失敗: {error}"),
            )
        })?;
    if duplicate.is_some() {
        return Err(api_error(StatusCode::CONFLICT, "座號不可重複"));
    }

    let updated = conn
        .execute(
            "UPDATE students
             SET seat_no_text = ?1,
                 nickname = ?2
             WHERE id = ?3 AND classroom_id = ?4",
            params![seat_no_text, body.nickname, student_id, classroom_id],
        )
        .map_err(|error| {
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("更新學生資料失敗: {error}"),
            )
        })?;
    if updated == 0 {
        return Err(api_error(StatusCode::NOT_FOUND, "指定學生不存在"));
    }

    let hub = {
        let guard = runtime.lock().await;
        guard.hub.clone()
    };
    {
        let mut guard = hub.lock().await;
        for session in guard.students.values_mut() {
            if session.student_id == student_id {
                session.seat_no_text = seat_no_text.clone();
                session.nickname = seat_nickname_display(&seat_no_text, &body.nickname);
            }
        }
    }

    broadcast_student_list(&hub).await;
    broadcast_classroom_state(&runtime).await;
    let payload = build_classroom_state(&runtime)
        .await
        .map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, error))?;
    Ok(Json(payload))
}

fn validate_group_no(group_no: i64) -> Result<i64, String> {
    if group_no < 0 {
        return Err("組別不可小於 0".to_string());
    }
    Ok(group_no)
}

async fn adjust_student_points_handler(
    State(state): State<HttpState>,
    Json(body): Json<UpdateStudentPointsRequest>,
) -> Result<Json<ClassroomStatePayload>, (StatusCode, Json<Value>)> {
    let runtime = state.runtime.clone();
    let (db_path, classroom_id) = {
        let guard = runtime.lock().await;
        (guard.db_path.clone(), guard.current_classroom_id)
    };

    let conn = Connection::open(db_path).map_err(|error| {
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("開啟資料庫失敗: {error}"),
        )
    })?;

    let updated = conn
        .execute(
            "UPDATE students
             SET points = COALESCE(points, 0) + ?1
             WHERE id = ?2 AND classroom_id = ?3",
            params![body.delta, body.student_id, classroom_id],
        )
        .map_err(|error| {
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("更新學生積分失敗: {error}"),
            )
        })?;

    if updated == 0 {
        return Err(api_error(StatusCode::NOT_FOUND, "指定學生不存在"));
    }

    broadcast_classroom_state(&runtime).await;
    let payload = build_classroom_state(&runtime)
        .await
        .map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, error))?;
    Ok(Json(payload))
}

async fn adjust_multiple_students_points_handler(
    State(state): State<HttpState>,
    Json(body): Json<UpdateMultipleStudentPointsRequest>,
) -> Result<Json<ClassroomStatePayload>, (StatusCode, Json<Value>)> {
    let runtime = state.runtime.clone();
    let (db_path, classroom_id) = {
        let guard = runtime.lock().await;
        (guard.db_path.clone(), guard.current_classroom_id)
    };

    if body.student_ids.is_empty() {
        let payload = build_classroom_state(&runtime)
            .await
            .map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, error))?;
        return Ok(Json(payload));
    }

    {
        let mut conn = Connection::open(db_path).map_err(|error| {
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("開啟資料庫失敗: {error}"),
            )
        })?;

        let tx = conn.transaction().map_err(|error| {
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("開啟事務失敗: {error}"),
            )
        })?;

        for student_id in &body.student_ids {
            tx.execute(
                "UPDATE students
                 SET points = COALESCE(points, 0) + ?1
                 WHERE id = ?2 AND classroom_id = ?3",
                params![body.delta, student_id, classroom_id],
            )
            .map_err(|error| {
                api_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("批次更新學生積分失敗: {error}"),
                )
            })?;
        }

        tx.commit().map_err(|error| {
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("提交事務失敗: {error}"),
            )
        })?;
    }

    broadcast_classroom_state(&runtime).await;
    let payload = build_classroom_state(&runtime)
        .await
        .map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, error))?;
    Ok(Json(payload))
}

async fn adjust_all_student_points_handler(
    State(state): State<HttpState>,
    Json(body): Json<UpdateAllStudentPointsRequest>,
) -> Result<Json<ClassroomStatePayload>, (StatusCode, Json<Value>)> {
    let runtime = state.runtime.clone();
    let (db_path, classroom_id) = {
        let guard = runtime.lock().await;
        (guard.db_path.clone(), guard.current_classroom_id)
    };

    let conn = Connection::open(db_path).map_err(|error| {
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("開啟資料庫失敗: {error}"),
        )
    })?;

    conn.execute(
        "UPDATE students
         SET points = COALESCE(points, 0) + ?1
         WHERE classroom_id = ?2",
        params![body.delta, classroom_id],
    )
    .map_err(|error| {
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("更新全班積分失敗: {error}"),
        )
    })?;

    broadcast_classroom_state(&runtime).await;
    let payload = build_classroom_state(&runtime)
        .await
        .map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, error))?;
    Ok(Json(payload))
}

async fn reset_all_student_points_handler(
    State(state): State<HttpState>,
) -> Result<Json<ClassroomStatePayload>, (StatusCode, Json<Value>)> {
    let runtime = state.runtime.clone();
    let (db_path, classroom_id) = {
        let guard = runtime.lock().await;
        (guard.db_path.clone(), guard.current_classroom_id)
    };

    let conn = Connection::open(db_path).map_err(|error| {
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("開啟資料庫失敗: {error}"),
        )
    })?;

    conn.execute(
        "UPDATE students
         SET points = 0
         WHERE classroom_id = ?1",
        params![classroom_id],
    )
    .map_err(|error| {
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("重設全班積分失敗: {error}"),
        )
    })?;

    broadcast_classroom_state(&runtime).await;
    let payload = build_classroom_state(&runtime)
        .await
        .map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, error))?;
    Ok(Json(payload))
}

async fn assign_student_group_handler(
    State(state): State<HttpState>,
    Json(body): Json<UpdateStudentGroupRequest>,
) -> Result<Json<ClassroomStatePayload>, (StatusCode, Json<Value>)> {
    let group_no = validate_group_no(body.group_no)
        .map_err(|error| api_error(StatusCode::BAD_REQUEST, error))?;
    let runtime = state.runtime.clone();
    let (db_path, classroom_id) = {
        let guard = runtime.lock().await;
        (guard.db_path.clone(), guard.current_classroom_id)
    };

    let conn = Connection::open(db_path).map_err(|error| {
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("開啟資料庫失敗: {error}"),
        )
    })?;

    let updated = conn
        .execute(
            "UPDATE students
             SET group_no = ?1
             WHERE id = ?2 AND classroom_id = ?3",
            params![group_no, body.student_id, classroom_id],
        )
        .map_err(|error| {
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("更新分組失敗: {error}"),
            )
        })?;

    if updated == 0 {
        return Err(api_error(StatusCode::NOT_FOUND, "指定學生不存在"));
    }

    broadcast_classroom_state(&runtime).await;
    let payload = build_classroom_state(&runtime)
        .await
        .map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, error))?;
    Ok(Json(payload))
}

async fn adjust_group_student_points_handler(
    State(state): State<HttpState>,
    Json(body): Json<UpdateGroupPointsRequest>,
) -> Result<Json<ClassroomStatePayload>, (StatusCode, Json<Value>)> {
    let group_no = validate_group_no(body.group_no)
        .map_err(|error| api_error(StatusCode::BAD_REQUEST, error))?;
    let runtime = state.runtime.clone();
    let (db_path, classroom_id) = {
        let guard = runtime.lock().await;
        (guard.db_path.clone(), guard.current_classroom_id)
    };

    let conn = Connection::open(db_path).map_err(|error| {
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("開啟資料庫失敗: {error}"),
        )
    })?;

    conn.execute(
        "UPDATE students
         SET points = COALESCE(points, 0) + ?1
         WHERE classroom_id = ?2 AND COALESCE(group_no, 0) = ?3",
        params![body.delta, classroom_id, group_no],
    )
    .map_err(|error| {
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("更新群組積分失敗: {error}"),
        )
    })?;

    broadcast_classroom_state(&runtime).await;
    let payload = build_classroom_state(&runtime)
        .await
        .map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, error))?;
    Ok(Json(payload))
}

fn find_roster_student_for_join(
    db_path: &PathBuf,
    classroom_id: i64,
    seat_no_text: &str,
) -> Result<Option<(i64, String, String)>, String> {
    let conn = Connection::open(db_path).map_err(|error| format!("開啟資料庫失敗: {error}"))?;
    conn.query_row(
        "SELECT id, seat_no_text, nickname
         FROM students
         WHERE classroom_id = ?1 AND seat_no_text = ?2",
        params![classroom_id, seat_no_text],
        |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        },
    )
    .optional()
    .map_err(|error| format!("查詢學生名單失敗: {error}"))
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
        send_ws_error(sender, "找不到可用目標端");
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
        send_ws_error(sender, "訊號傳遞失敗，目標端可能已離線");
    }
}

async fn handle_socket(
    socket: WebSocket,
    runtime: Arc<Mutex<BackendRuntime>>,
    hub: Arc<Mutex<SessionHub>>,
    role: String,
) {
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

        let teacher_ready_for_students = SignalEnvelope {
            event: "teacher-ready".to_string(),
            source: Some(connection_id.clone()),
            target: None,
            nickname: None,
            payload: None,
            message: None,
        };

        if let Ok(raw) = serde_json::to_string(&teacher_ready_for_students) {
            let mut stale_students = Vec::new();
            for (student_id, tx) in &guard.student_channels {
                if tx.send(raw.clone()).is_err() {
                    stale_students.push(student_id.clone());
                }
            }

            for student_id in stale_students {
                guard.student_channels.remove(&student_id);
                guard.students.remove(&student_id);
            }
        }

        drop(guard);
        if let Ok(state) = build_classroom_state(&runtime).await {
            send_json(
                &out_tx,
                &SignalEnvelope {
                    event: "classroom-state".to_string(),
                    source: None,
                    target: None,
                    nickname: None,
                    payload: Some(json!({ "state": state })),
                    message: None,
                },
            );
        }
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
        drop(guard);
        if let Ok(state) = build_classroom_state(&runtime).await {
            send_json(
                &out_tx,
                &SignalEnvelope {
                    event: "classroom-state".to_string(),
                    source: None,
                    target: None,
                    nickname: None,
                    payload: Some(json!({ "state": state })),
                    message: None,
                },
            );
        }
    } else if let Ok(state) = build_classroom_state(&runtime).await {
        send_json(
            &out_tx,
            &SignalEnvelope {
                event: "classroom-state".to_string(),
                source: None,
                target: None,
                nickname: None,
                payload: Some(json!({ "state": state })),
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
            send_ws_error(&out_tx, "訊息格式錯誤");
            continue;
        };

        match incoming.event.as_str() {
            "force-logout-all-students" if is_teacher => {
                let classroom_id = {
                    let guard = runtime.lock().await;
                    guard.current_classroom_id
                };

                force_logout_classroom_students(
                    &hub,
                    classroom_id,
                    "teacher-force-logout",
                    "教師已要求所有學生重新選擇座號加入",
                )
                .await;

                broadcast_student_list(&hub).await;
                broadcast_classroom_state(&runtime).await;
            }
            "join" if !is_teacher && !is_console => {
                let Some(payload) = incoming.payload.clone() else {
                    send_ws_error(&out_tx, "缺少班級與座號資訊");
                    continue;
                };

                let join_payload = match serde_json::from_value::<JoinPayload>(payload) {
                    Ok(payload) => payload,
                    Err(_) => {
                        send_ws_error(&out_tx, "加入資料格式錯誤");
                        continue;
                    }
                };

                let seat_no_text = normalize_seat_no_text(&join_payload.seat_no_text);
                if seat_no_text.is_empty() {
                    send_ws_error(&out_tx, "請選擇有效座號");
                    continue;
                }

                let (db_path, current_classroom_id) = {
                    let guard = runtime.lock().await;
                    (guard.db_path.clone(), guard.current_classroom_id)
                };

                if join_payload.classroom_id != current_classroom_id {
                    send_ws_error(&out_tx, "班級已變更，請重新選擇名單");
                    continue;
                }

                let student_row = match find_roster_student_for_join(
                    &db_path,
                    current_classroom_id,
                    &seat_no_text,
                ) {
                    Ok(Some(student_row)) => student_row,
                    Ok(None) => {
                        send_ws_error(&out_tx, "找不到對應學生名單項目");
                        continue;
                    }
                    Err(error) => {
                        send_ws_error(&out_tx, error);
                        continue;
                    }
                };
                let (student_row_id, student_seat_no_text, student_nickname) = student_row;
                let display_name = seat_nickname_display(&student_seat_no_text, &student_nickname);

                let seat_is_occupied = {
                    let guard = hub.lock().await;
                    guard.students.values().any(|student| {
                        student.classroom_id == current_classroom_id
                            && student.seat_no_text == student_seat_no_text
                    })
                };
                if seat_is_occupied {
                    send_ws_error(&out_tx, "該學生已連入，請選擇其他座號");
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
                            student_id: student_row_id,
                            classroom_id: current_classroom_id,
                            seat_no_text: student_seat_no_text.clone(),
                            nickname: display_name.clone(),
                            connected: true,
                            focus_status: "focused".to_string(),
                            focus_updated_at: 0,
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
                        nickname: Some(display_name),
                        payload: None,
                        message: None,
                    },
                );

                broadcast_student_list(&hub).await;
                broadcast_classroom_state(&runtime).await;
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
        broadcast_classroom_state(&runtime).await;
    }

    writer_task.abort();
}

const LINE_RICHMENU_MAX_ACTION_TEXT: usize = 300;
const LINE_API_BASE: &str = "https://api.line.me/v2/bot";
const LINE_API_DATA_BASE: &str = "https://api-data.line.me/v2/bot";

#[derive(Debug, Deserialize)]
struct SyncTaskItem {
    title: String,
    task_date: String,
}

#[derive(Debug, Deserialize)]
struct SyncToLineRequest {
    tasks: Vec<SyncTaskItem>,
}

#[derive(Debug, Serialize)]
struct SyncToLineResponse {
    success: bool,
    rich_menu_id: String,
    message: String,
    summarized: bool,
}

#[derive(Debug, Serialize)]
struct LineRichMenuItem {
    rich_menu_id: String,
    name: String,
    chat_bar_text: String,
    selected: bool,
}

fn get_line_client_and_token(
    conn: &Connection,
    classroom_id: i64,
) -> Result<(reqwest::Client, String), (StatusCode, Json<Value>)> {
    let classroom = conn
        .query_row(
            "SELECT line_enabled, line_channel_access_token \
             FROM classrooms WHERE id = ?1",
            params![classroom_id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                ))
            },
        )
        .optional()
        .map_err(|error| {
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("讀取班級資訊失敗: {error}"),
            )
        })?
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "指定班級不存在"))?;

    let (enabled, token) = classroom;

    if enabled == 0 {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "該班級未啟用 LINE 同步功能",
        ));
    }

    if token.is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "該班級未設定 LINE Channel Access Token",
        ));
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|error| {
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("建立 HTTP client 失敗: {error}"),
            )
        })?;

    Ok((client, token))
}

async fn list_line_richmenus_handler(
    State(state): State<HttpState>,
    Path(classroom_id): Path<i64>,
) -> Result<Json<Vec<LineRichMenuItem>>, (StatusCode, Json<Value>)> {
    let runtime = state.runtime.clone();
    let db_path = { runtime.lock().await.db_path.clone() };
    let conn = Connection::open(db_path).map_err(|error| {
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("開啟資料庫失敗: {error}"),
        )
    })?;

    let (client, token) = get_line_client_and_token(&conn, classroom_id)?;
    let auth_header = format!("Bearer {}", token);

    let resp = client
        .get(format!("{}/richmenu/list", LINE_API_BASE))
        .header("Authorization", &auth_header)
        .send()
        .await
        .map_err(|error| {
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("查詢 LINE rich menu 列表失敗: {error}"),
            )
        })?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(api_error(
            StatusCode::BAD_GATEWAY,
            format!("LINE API 查詢 rich menu 列表錯誤：HTTP {status}: {body}"),
        ));
    }

    let list_body: Value = resp.json().await.map_err(|error| {
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("解析 LINE API 回覆失敗: {error}"),
        )
    })?;

    let menus = list_body["richmenus"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|item| LineRichMenuItem {
                    rich_menu_id: item["richMenuId"]
                        .as_str()
                        .unwrap_or("")
                        .to_string(),
                    name: item["name"].as_str().unwrap_or("").to_string(),
                    chat_bar_text: item["chatBarText"]
                        .as_str()
                        .unwrap_or("")
                        .to_string(),
                    selected: item["selected"].as_bool().unwrap_or(false),
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Ok(Json(menus))
}

async fn delete_line_richmenu_handler(
    State(state): State<HttpState>,
    Path((classroom_id, rich_menu_id)): Path<(i64, String)>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let runtime = state.runtime.clone();
    let db_path = { runtime.lock().await.db_path.clone() };
    let conn = Connection::open(db_path).map_err(|error| {
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("開啟資料庫失敗: {error}"),
        )
    })?;

    let (client, token) = get_line_client_and_token(&conn, classroom_id)?;
    let auth_header = format!("Bearer {}", token);

    let resp = client
        .delete(format!("{}/richmenu/{}", LINE_API_BASE, rich_menu_id))
        .header("Authorization", &auth_header)
        .send()
        .await
        .map_err(|error| {
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("刪除 rich menu 失敗: {error}"),
            )
        })?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(api_error(
            StatusCode::BAD_GATEWAY,
            format!("LINE API 拒絕刪除 rich menu：HTTP {status}: {body}"),
        ));
    }

    // Also clear the stored rich_menu_id if it matches
    let _ = conn.execute(
        "UPDATE classrooms SET line_rich_menu_id = '' \
         WHERE id = ?1 AND line_rich_menu_id = ?2",
        params![classroom_id, rich_menu_id],
    );

    Ok(Json(json!({ "success": true })))
}

fn generate_rich_menu_json(sync_text: &str) -> Value {
    json!({
        "size": { "width": 2500, "height": 1686 },
        "selected": true,
        "name": "聯絡簿同步",
        "chatBarText": "聯絡簿",
        "areas": [
            {
                "bounds": { "x": 0, "y": 0, "width": 2500, "height": 1686 },
                "action": {
                    "type": "message",
                    "text": sync_text
                }
            }
        ]
    })
}

const DEFAULT_RICHMENU_PNG: &[u8] =
    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/default_richmenu.png"));

fn format_date_weekday(iso_date: &str) -> String {
    if let Ok(d) = chrono::NaiveDate::parse_from_str(iso_date, "%Y-%m-%d") {
        match d.format("%w").to_string().as_str() {
            "0" => format!("{}/{} (日)", d.month(), d.day()),
            "1" => format!("{}/{} (一)", d.month(), d.day()),
            "2" => format!("{}/{} (二)", d.month(), d.day()),
            "3" => format!("{}/{} (三)", d.month(), d.day()),
            "4" => format!("{}/{} (四)", d.month(), d.day()),
            "5" => format!("{}/{} (五)", d.month(), d.day()),
            "6" => format!("{}/{} (六)", d.month(), d.day()),
            _ => iso_date.to_string(),
        }
    } else {
        iso_date.to_string()
    }
}

fn build_sync_text(tasks: &[SyncTaskItem]) -> (String, bool) {
    let all_same_date = tasks
        .first()
        .map(|first| tasks.iter().all(|t| t.task_date == first.task_date))
        .unwrap_or(true);

    let mut text = if all_same_date {
        if let Some(first) = tasks.first() {
            format!(
                "=== 聯絡簿 ({}) ===\n",
                format_date_weekday(&first.task_date)
            )
        } else {
            "=== 聯絡簿 ===\n".to_string()
        }
    } else {
        "=== 聯絡簿 ===\n".to_string()
    };

    let mut current_date = String::new();
    for (i, task) in tasks.iter().enumerate() {
        if !all_same_date && task.task_date != current_date {
            let date_line = format!("{}\n", format_date_weekday(&task.task_date));
            if text.len() + date_line.len() > LINE_RICHMENU_MAX_ACTION_TEXT {
                text.push_str(&format!("…等共 {} 項", tasks.len()));
                return (text, true);
            }
            text.push_str(&date_line);
            current_date = task.task_date.clone();
        }

        let truncated: String = task.title.chars().take(60).collect();
        let line = if task.title.len() > 180 {
            format!("{}. {}…\n", i + 1, truncated)
        } else {
            format!("{}. {}\n", i + 1, task.title)
        };
        if text.len() + line.len() > LINE_RICHMENU_MAX_ACTION_TEXT {
            text.push_str(&format!("…等共 {} 項", tasks.len()));
            return (text, true);
        }
        text.push_str(&line);
    }
    (text, false)
}

async fn sync_to_line_handler(
    State(state): State<HttpState>,
    Path(classroom_id): Path<i64>,
    Json(body): Json<SyncToLineRequest>,
) -> Result<Json<SyncToLineResponse>, (StatusCode, Json<Value>)> {
    if body.tasks.is_empty() {
        return Err(api_error(StatusCode::BAD_REQUEST, "同步內容不可為空"));
    }

    let runtime = state.runtime.clone();
    let db_path = {
        let guard = runtime.lock().await;
        guard.db_path.clone()
    };
    let conn = Connection::open(db_path).map_err(|error| {
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("開啟資料庫失敗: {error}"),
        )
    })?;

    let classroom = conn
        .query_row(
            "SELECT id, line_enabled, line_channel_access_token, line_channel_secret, \
             line_rich_menu_id FROM classrooms WHERE id = ?1",
            params![classroom_id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            },
        )
        .optional()
        .map_err(|error| {
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("讀取班級資訊失敗: {error}"),
            )
        })?
        .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "指定班級不存在"))?;

    let (_id, enabled, token, _secret, old_rich_menu_id) = classroom;

    if enabled == 0 {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "該班級未啟用 LINE 同步功能，請先在班級設定中啟用",
        ));
    }

    if token.is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "該班級未設定 LINE Channel Access Token",
        ));
    }

    let (sync_text, summarized) = build_sync_text(&body.tasks);
    let rich_menu_json = generate_rich_menu_json(&sync_text);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|error| {
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("建立 HTTP client 失敗: {error}"),
            )
        })?;

    let auth_header = format!("Bearer {}", token);

    if !old_rich_menu_id.is_empty() {
        let _ = client
            .delete(format!(
                "{}/richmenu/{}",
                LINE_API_BASE, old_rich_menu_id
            ))
            .header("Authorization", &auth_header)
            .send()
            .await;
    }

    let create_resp = client
        .post(format!("{}/richmenu", LINE_API_BASE))
        .header("Authorization", &auth_header)
        .json(&rich_menu_json)
        .send()
        .await
        .map_err(|error| {
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("呼叫 LINE API 建立 rich menu 失敗: {error}"),
            )
        })?;

    if !create_resp.status().is_success() {
        let status = create_resp.status();
        let error_body = create_resp.text().await.unwrap_or_default();
        let detail = if error_body.is_empty() {
            format!("HTTP {status}（無錯誤內容）")
        } else if let Ok(json) = serde_json::from_str::<Value>(&error_body) {
            if let Some(msg) = json.get("message").and_then(|v| v.as_str()) {
                format!("HTTP {status}: {msg}")
            } else {
                format!("HTTP {status}: {error_body}")
            }
        } else {
            format!("HTTP {status}: {error_body}")
        };
        return Err(api_error(
            StatusCode::BAD_GATEWAY,
            format!("LINE API 建立 rich menu 錯誤：{detail}"),
        ));
    }

    let create_body: Value = create_resp.json().await.map_err(|error| {
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("解析 LINE API 回覆失敗: {error}"),
        )
    })?;

    let new_rich_menu_id = create_body["richMenuId"]
        .as_str()
        .ok_or_else(|| api_error(StatusCode::INTERNAL_SERVER_ERROR, "LINE API 未回傳 richMenuId"))?
        .to_string();

    let png_data = DEFAULT_RICHMENU_PNG.to_vec();
    let img_resp = client
        .post(format!(
            "{}/richmenu/{}/content",
            LINE_API_DATA_BASE, new_rich_menu_id
        ))
        .header("Authorization", &auth_header)
        .header("Content-Type", "image/png")
        .body(png_data)
        .send()
        .await
        .map_err(|error| {
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("上傳 rich menu 圖片失敗: {error}"),
            )
        })?;

    if !img_resp.status().is_success() {
        let status = img_resp.status();
        let error_body = img_resp.text().await.unwrap_or_default();
        let detail = if error_body.is_empty() {
            format!("HTTP {status}（無錯誤內容）")
        } else if let Ok(json) = serde_json::from_str::<Value>(&error_body) {
            if let Some(msg) = json.get("message").and_then(|v| v.as_str()) {
                format!("HTTP {status}: {msg}")
            } else {
                format!("HTTP {status}: {error_body}")
            }
        } else {
            format!("HTTP {status}: {error_body}")
        };
        return Err(api_error(
            StatusCode::BAD_GATEWAY,
            format!("LINE API 拒絕 rich menu 圖片：{detail}"),
        ));
    }

    let default_resp = client
        .post(format!(
            "{}/user/all/richmenu/{}",
            LINE_API_BASE, new_rich_menu_id
        ))
        .header("Authorization", &auth_header)
        .header("Content-Length", "0")
        .send()
        .await
        .map_err(|error| {
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("設定預設 rich menu 失敗: {error}"),
            )
        })?;

    if !default_resp.status().is_success() {
        let status = default_resp.status();
        let error_body = default_resp.text().await.unwrap_or_default();
        let detail = if error_body.is_empty() {
            format!("HTTP {status}（無錯誤內容）")
        } else if let Ok(json) = serde_json::from_str::<Value>(&error_body) {
            if let Some(msg) = json.get("message").and_then(|v| v.as_str()) {
                format!("HTTP {status}: {msg}")
            } else {
                format!("HTTP {status}: {error_body}")
            }
        } else {
            format!("HTTP {status}: {error_body}")
        };
        return Err(api_error(
            StatusCode::BAD_GATEWAY,
            format!("LINE API 拒絕設定預設 rich menu：{detail}"),
        ));
    }

    if let Err(error) = conn.execute(
        "UPDATE classrooms SET line_rich_menu_id = ?1 WHERE id = ?2",
        params![new_rich_menu_id, classroom_id],
    ) {
        eprintln!("[line-sync] 儲存 richMenuId 失敗: {error}");
    }

    broadcast_classroom_state(&runtime).await;

    Ok(Json(SyncToLineResponse {
        success: true,
        rich_menu_id: new_rich_menu_id,
        message: if summarized {
            format!("同步成功（已自動摘要為 {} 字）", sync_text.len())
        } else {
            "同步成功".to_string()
        },
        summarized,
    }))
}

async fn start_server_impl(
    runtime: Arc<Mutex<BackendRuntime>>,
    runtime_app_version: String,
) -> Result<ServerInfo, String> {
    let (port, hub, frontend_assets_root) = {
        let mut guard = runtime.lock().await;
        if guard.running.is_some() {
            return Ok(guard.control.to_info());
        }
        guard.control.status = ServiceStatus::Starting;
        guard.control.error = None;
        guard.control.refresh_ip_url();
        (
            guard.control.port,
            guard.hub.clone(),
            guard.frontend_assets_root.clone(),
        )
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
    let root_assets = ServeDir::new(frontend_assets_root.join("assets"));
    let local_ip_host = format_host_for_url(&resolve_local_ip());
    let local_dev_origin = format!("http://{local_ip_host}:1420");
    let mut allowed_origins: Vec<HeaderValue> = vec![
        "http://localhost:1420"
            .parse()
            .expect("valid dev webview origin"),
        "http://127.0.0.1:1420"
            .parse()
            .expect("valid dev webview origin"),
        "http://localhost:17860"
            .parse()
            .expect("valid app web origin"),
        "http://127.0.0.1:17860"
            .parse()
            .expect("valid app web origin"),
        "tauri://localhost"
            .parse()
            .expect("valid tauri webview origin"),
        "http://tauri.localhost"
            .parse()
            .expect("valid tauri webview origin"),
        "https://tauri.localhost"
            .parse()
            .expect("valid tauri webview origin"),
    ];
    if let Ok(origin) = local_dev_origin.parse::<HeaderValue>() {
        allowed_origins.push(origin);
    }
    if let Ok(origin) = format!("http://{local_ip_host}:17860").parse::<HeaderValue>() {
        allowed_origins.push(origin);
    }
    let cors_layer = CorsLayer::new()
        .allow_origin(allowed_origins)
        .allow_methods(Any)
        .allow_headers(Any);
    let mut router = Router::new()
        .route("/", get(student_page))
        .route("/student", get(student_page))
        .route("/teacher", get(teacher_page))
        .route("/api/app/version", get(app_version))
        .route("/api/classroom/state", get(get_classroom_state_handler))
        .route(
            "/api/classrooms",
            get(list_classrooms_handler).post(create_classroom_handler),
        )
        .route("/api/classrooms/select", post(select_classroom_handler))
        .route(
            "/api/classrooms/{classroom_id}",
            patch(update_classroom_handler).delete(delete_classroom_handler),
        )
        .route(
            "/api/classrooms/{classroom_id}/students/{student_id}",
            patch(update_student_handler),
        )
        .route(
            "/api/classrooms/{classroom_id}/students/bulk-save",
            post(save_class_members_handler),
        )
        .route(
            "/api/student-points/adjust-student",
            post(adjust_student_points_handler),
        )
        .route(
            "/api/student-points/adjust-multiple",
            post(adjust_multiple_students_points_handler),
        )
        .route(
            "/api/student-points/adjust-all",
            post(adjust_all_student_points_handler),
        )
        .route(
            "/api/student-points/reset-all",
            post(reset_all_student_points_handler),
        )
        .route(
            "/api/student-points/assign-group",
            post(assign_student_group_handler),
        )
        .route(
            "/api/student-points/adjust-group",
            post(adjust_group_student_points_handler),
        )
        .route(
            "/api/contact-book/tasks",
            get(list_contact_book_tasks_handler).post(create_contact_book_task_handler),
        )
        .route(
            "/api/contact-book/tasks/{task_id}",
            patch(update_contact_book_task_handler).delete(delete_contact_book_task_handler),
        )
        .route(
            "/api/contact-book/tasks/{task_id}/submissions",
            get(list_task_submissions_handler),
        )
        .route(
            "/api/contact-book/tasks/{task_id}/submissions/{student_id}",
            patch(update_task_submission_handler),
        )
        .route(
            "/api/contact-book/tasks/{task_id}/completion",
            post(set_task_completion_handler),
        )
        .route(
            "/api/contact-book/sync-to-line/{classroom_id}",
            post(sync_to_line_handler),
        )
        .route(
            "/api/contact-book/line-richmenus/{classroom_id}",
            get(list_line_richmenus_handler),
        )
        .route(
            "/api/contact-book/line-richmenus/{classroom_id}/{rich_menu_id}",
            delete(delete_line_richmenu_handler),
        )
        .route(
            "/api/reminder-boards",
            get(get_reminder_boards_handler).post(create_reminder_board_handler),
        )
        .route(
            "/api/reminder-boards/{id}",
            patch(update_reminder_board_handler).delete(delete_reminder_board_handler),
        )
        .route("/health", get(health))
        .route_service(
            "/song-class.png",
            ServeFile::new(frontend_assets_root.join("song-class.png")),
        )
        .route("/ws", get(ws_handler))
        .nest_service("/assets", root_assets)
        .layer(cors_layer)
        .with_state(HttpState {
            runtime: runtime.clone(),
            hub,
            app_version: runtime_app_version,
        });

    if cfg!(debug_assertions) {
        router = router
            .route("/app", get(app_page))
            .route("/app/", get(app_page));
    } else {
        let app_assets = ServeDir::new(frontend_assets_root.clone())
            .not_found_service(ServeFile::new(frontend_assets_root.join("index.html")));
        router = router.nest_service("/app", app_assets);
    }

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
    start_server_impl(state.inner.clone(), state.app_version.clone()).await
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

#[tauri::command]
async fn backup_database(
    state: tauri::State<'_, BackendState>,
    destination_dir: String,
) -> Result<String, String> {
    let db_path = {
        let guard = state.inner.lock().await;
        guard.db_path.clone()
    };

    if !db_path.exists() {
        return Err("目前資料庫檔案不存在，無法備份。".to_string());
    }

    let destination = PathBuf::from(destination_dir);
    fs::create_dir_all(&destination).map_err(|error| format!("建立備份目錄失敗: {error}"))?;

    let timestamp = Local::now().format("%Y%m%d%H%M%S").to_string();
    let backup_path = destination.join(format!("song-class-backup-{timestamp}.sqlite3"));

    fs::copy(&db_path, &backup_path).map_err(|error| format!("備份資料庫失敗: {error}"))?;

    Ok(backup_path.to_string_lossy().to_string())
}

#[tauri::command]
async fn restore_database(
    state: tauri::State<'_, BackendState>,
    source_path: String,
) -> Result<(), String> {
    let source = PathBuf::from(source_path);
    let db_path = {
        let guard = state.inner.lock().await;
        guard.db_path.clone()
    };

    if !source.exists() {
        return Err("選擇的資料庫檔案不存在。".to_string());
    }

    if source == db_path {
        return Err("不能回存到目前正在使用的資料庫檔案。".to_string());
    }

    let _ = stop_server_impl(state.inner.clone()).await?;

    fs::copy(&source, &db_path).map_err(|error| format!("回存資料庫失敗: {error}"))?;

    let current_classroom_id = init_database(&db_path)?;

    {
        let mut guard = state.inner.lock().await;
        guard.current_classroom_id = current_classroom_id;
    }

    let runtime = state.inner.clone();
    let app_version = state.app_version.clone();
    tauri::async_runtime::spawn(async move {
        let _ = start_server_impl(runtime, app_version).await;
    });

    Ok(())
}

#[tauri::command]
async fn get_server_debug_info(
    state: tauri::State<'_, BackendState>,
) -> Result<ServerDebugInfo, String> {
    let mut guard = state.inner.lock().await;
    guard.control.refresh_ip_url();

    let base_url = guard.control.url.clone();
    let frontend_assets_root = guard.frontend_assets_root.clone();
    let frontend_assets_candidates = guard.frontend_assets_candidates.clone();
    let tauri_resource_dir = guard.tauri_resource_dir.clone();
    let frontend_assets_root_text = frontend_assets_root.to_string_lossy().to_string();
    let executable_path = std::env::current_exe()
        .ok()
        .map(|path| path.to_string_lossy().to_string());

    Ok(ServerDebugInfo {
        frontend_assets_root: frontend_assets_root_text,
        frontend_index_exists: frontend_assets_root.join("index.html").is_file(),
        frontend_assets_dir_exists: frontend_assets_root.join("assets").is_dir(),
        checked_frontend_paths: frontend_assets_candidates
            .iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect(),
        executable_path,
        tauri_resource_dir,
        app_teacher_url: format!("http://localhost:{DEFAULT_PORT}/teacher?base={base_url}"),
        app_student_url: format!("{base_url}/student"),
        teacher_redirect_url: format!("http://localhost:{DEFAULT_PORT}/teacher?base={base_url}"),
        student_redirect_url: format!("{base_url}/student"),
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReminderBoard {
    id: Option<i64>,
    category: String,
    title: String,
    subtitle: String,
    icon: String,
}

#[tauri::command]
async fn get_reminder_boards(
    state: tauri::State<'_, BackendState>,
) -> Result<Vec<ReminderBoard>, String> {
    let runtime = state.inner.lock().await;
    let conn = Connection::open(&runtime.db_path).map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare("SELECT id, category, title, subtitle, icon FROM reminder_boards WHERE category = '自訂' ORDER BY id DESC")
        .map_err(|e| e.to_string())?;

    let boards = stmt
        .query_map([], |row| {
            Ok(ReminderBoard {
                id: Some(row.get(0)?),
                category: row.get(1)?,
                title: row.get(2)?,
                subtitle: row.get(3)?,
                icon: row.get(4)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;

    Ok(boards)
}

#[tauri::command]
async fn create_reminder_board(
    state: tauri::State<'_, BackendState>,
    board: ReminderBoard,
) -> Result<ReminderBoard, String> {
    let runtime = state.inner.lock().await;
    let conn = Connection::open(&runtime.db_path).map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT INTO reminder_boards (category, title, subtitle, icon) VALUES (?1, ?2, ?3, ?4)",
        params![board.category, board.title, board.subtitle, board.icon],
    )
    .map_err(|e| e.to_string())?;

    let id = conn.last_insert_rowid();
    let mut new_board = board;
    new_board.id = Some(id);
    Ok(new_board)
}

#[tauri::command]
async fn update_reminder_board(
    state: tauri::State<'_, BackendState>,
    board: ReminderBoard,
) -> Result<(), String> {
    let id = board.id.ok_or("缺少 ID")?;
    let runtime = state.inner.lock().await;
    let conn = Connection::open(&runtime.db_path).map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE reminder_boards SET title = ?1, subtitle = ?2, icon = ?3 WHERE id = ?4",
        params![board.title, board.subtitle, board.icon, id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
async fn delete_reminder_board(
    state: tauri::State<'_, BackendState>,
    id: i64,
) -> Result<(), String> {
    let runtime = state.inner.lock().await;
    let conn = Connection::open(&runtime.db_path).map_err(|e| e.to_string())?;
    conn.execute("DELETE FROM reminder_boards WHERE id = ?1", params![id])
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn get_app_version(state: tauri::State<'_, BackendState>) -> String {
    state.app_version.clone()
}

fn collect_frontend_assets_candidates(app: &tauri::AppHandle) -> Vec<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();

    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        candidates.push(PathBuf::from(manifest_dir).join("../dist"));
    }

    if let Ok(current_dir) = std::env::current_dir() {
        candidates.push(current_dir.join("dist"));
        candidates.push(current_dir.join("../dist"));
    }

    if let Ok(resource_dir) = app.path().resource_dir() {
        candidates.push(resource_dir.join("dist"));
        candidates.push(resource_dir.join("_up_/dist"));
        candidates.push(resource_dir);
    }

    if let Ok(executable_path) = std::env::current_exe() {
        if let Some(executable_dir) = executable_path.parent() {
            // Linux deb/rpm install usually places binary in /usr/bin and app assets in /usr/lib/<app>/_up_/dist.
            candidates.push(executable_dir.join("../lib/song-class/_up_/dist"));
            candidates.push(executable_dir.join("../lib/song-class/dist"));
            candidates.push(executable_dir.join("_up_/dist"));
            candidates.push(executable_dir.join("dist"));
        }
    }

    candidates.push(PathBuf::from("/usr/lib/song-class/_up_/dist"));

    candidates
}

fn resolve_frontend_assets_root(candidates: &[PathBuf]) -> PathBuf {
    for candidate in candidates {
        if candidate.join("index.html").is_file() {
            return candidate.clone();
        }
    }

    if let Some(first_candidate) = candidates.first() {
        return first_candidate.clone();
    }

    PathBuf::from("../dist")
}

fn restore_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let backup_item =
                MenuItem::with_id(app, "backup-database", "備份資料庫", true, None::<&str>)?;
            let restore_item =
                MenuItem::with_id(app, "restore-database", "回存資料庫", true, None::<&str>)?;
            let file_menu = SubmenuBuilder::new(app, "檔案")
                .item(&backup_item)
                .item(&restore_item)
                .build()?;
            let menu = MenuBuilder::new(app).item(&file_menu).build()?;
            app.set_menu(menu)?;

            let show_item =
                MenuItem::with_id(app, "show-main-window", "顯示主視窗", true, None::<&str>)?;
            let exit_item = MenuItem::with_id(app, "exit", "結束程式", true, None::<&str>)?;
            let tray_menu = Menu::with_items(app, &[&show_item, &exit_item])?;

            let mut tray_builder = TrayIconBuilder::with_id("main-tray")
                .menu(&tray_menu)
                .show_menu_on_left_click(true)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "show-main-window" => restore_main_window(app),
                    "exit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    let app = tray.app_handle();
                    match event {
                        TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Down,
                            ..
                        }
                        | TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        }
                        | TrayIconEvent::DoubleClick {
                            button: MouseButton::Left,
                            ..
                        } => {
                            restore_main_window(&app);
                        }
                        _ => {}
                    }
                });

            if let Some(tray_icon) = app.default_window_icon().cloned() {
                tray_builder = tray_builder.icon(tray_icon);
            }

            app.on_menu_event(|app, event| match event.id().as_ref() {
                "backup-database" => {
                    let _ = app.emit("menu-backup-database", ());
                }
                "restore-database" => {
                    let _ = app.emit("menu-restore-database", ());
                }
                _ => {}
            });

            tray_builder.build(app)?;

            let tauri_resource_dir = app
                .path()
                .resource_dir()
                .ok()
                .map(|path| path.to_string_lossy().to_string());
            let frontend_assets_candidates = collect_frontend_assets_candidates(app.handle());
            let frontend_assets_root = resolve_frontend_assets_root(&frontend_assets_candidates);

            let db_dir = app
                .path()
                .app_data_dir()
                .ok()
                .or_else(|| std::env::current_dir().ok().map(|path| path.join("data")))
                .unwrap_or_else(|| PathBuf::from("./data"));
            let db_path = db_dir.join("song-class.sqlite3");
            let current_classroom_id = init_database(&db_path)
                .map_err(|error| std::io::Error::new(std::io::ErrorKind::Other, error))?;
            let runtime_app_version = resolve_app_version(&app);

            app.manage(BackendState::new(
                runtime_app_version.clone(),
                frontend_assets_root,
                frontend_assets_candidates,
                tauri_resource_dir,
                db_path,
                current_classroom_id,
            ));
            let backend_state = app.state::<BackendState>();
            let runtime = backend_state.inner.clone();
            let app_version = backend_state.app_version.clone();
            tauri::async_runtime::spawn(async move {
                let _ = start_server_impl(runtime, app_version).await;
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            start_server,
            stop_server,
            get_server_info,
            backup_database,
            restore_database,
            get_server_debug_info,
            get_app_version,
            get_reminder_boards,
            create_reminder_board,
            update_reminder_board,
            delete_reminder_board
        ])
        .on_window_event(|window, event| {
            if window.label() != "main" {
                return;
            }

            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_temp_db_path() -> PathBuf {
        std::env::temp_dir().join(format!("song-class-test-{}.sqlite3", Uuid::new_v4()))
    }

    #[test]
    fn init_database_creates_default_class_and_30_students() {
        let db_path = make_temp_db_path();
        let class_id = init_database(&db_path).expect("init database should succeed");
        assert!(class_id > 0, "current classroom id should be positive");

        let conn = Connection::open(&db_path).expect("open sqlite database");
        let class_count: i64 = conn
            .query_row("SELECT COUNT(1) FROM classrooms", [], |row| row.get(0))
            .expect("query classroom count");
        assert_eq!(
            class_count, 1,
            "should create exactly one default classroom"
        );

        let student_count: i64 = conn
            .query_row(
                "SELECT COUNT(1) FROM students WHERE classroom_id = ?1",
                params![class_id],
                |row| row.get(0),
            )
            .expect("query student count");
        assert_eq!(student_count, 30, "should create 30 default students");

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn init_database_is_idempotent_without_duplicate_seed_data() {
        let db_path = make_temp_db_path();

        let first_class_id = init_database(&db_path).expect("first init should succeed");
        let second_class_id = init_database(&db_path).expect("second init should succeed");
        assert_eq!(
            first_class_id, second_class_id,
            "current classroom id should stay stable"
        );

        let conn = Connection::open(&db_path).expect("open sqlite database");
        let class_count: i64 = conn
            .query_row("SELECT COUNT(1) FROM classrooms", [], |row| row.get(0))
            .expect("query classroom count");
        assert_eq!(class_count, 1, "should not create duplicate classrooms");

        let duplicated_seat_count: i64 = conn
            .query_row(
                "SELECT COUNT(1)
                 FROM (
                   SELECT seat_no_text
                   FROM students
                   WHERE classroom_id = ?1
                   GROUP BY seat_no_text
                   HAVING COUNT(1) > 1
                 )",
                params![first_class_id],
                |row| row.get(0),
            )
            .expect("query duplicate seat count");
        assert_eq!(
            duplicated_seat_count, 0,
            "seat numbers should remain unique"
        );

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn init_database_sets_schema_version_to_latest() {
        let db_path = make_temp_db_path();

        init_database(&db_path).expect("init should set schema version");

        let conn = Connection::open(&db_path).expect("open sqlite database");
        let user_version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("query user_version");
        assert_eq!(
            user_version,
            latest_schema_version() as i64,
            "schema version should match latest"
        );

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn init_database_adopts_legacy_schema_without_reseeding() {
        let db_path = make_temp_db_path();
        let conn = Connection::open(&db_path).expect("open sqlite database");
        let (_, baseline_sql) = baseline_migration().expect("read baseline migration");
        conn.execute_batch(baseline_sql)
            .expect("apply legacy v0 sql");
        conn.execute(
            "INSERT INTO classrooms (name) VALUES (?1)",
            params!["既有班級"],
        )
        .expect("insert legacy classroom");
        drop(conn);

        init_database(&db_path).expect("init should adopt legacy schema");

        let conn = Connection::open(&db_path).expect("open sqlite database");
        let user_version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("query user_version");
        assert_eq!(user_version, latest_schema_version() as i64);

        let class_count: i64 = conn
            .query_row("SELECT COUNT(1) FROM classrooms", [], |row| row.get(0))
            .expect("query classroom count");
        assert_eq!(class_count, 1, "should not reseed existing classrooms");

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn student_points_sql_updates_support_single_all_reset_and_group_ops() {
        let db_path = make_temp_db_path();
        init_database(&db_path).expect("init should succeed");

        let conn = Connection::open(&db_path).expect("open sqlite database");
        let class_id: i64 = conn
            .query_row("SELECT id FROM classrooms LIMIT 1", [], |row| row.get(0))
            .expect("query classroom id");

        conn.execute_batch(include_str!("../sql/migrations/next_release.sql"))
            .expect("apply student-points migration draft");

        let first_student_id: i64 = conn
            .query_row(
                "SELECT id FROM students WHERE classroom_id = ?1 ORDER BY seat_no_text ASC LIMIT 1",
                params![class_id],
                |row| row.get(0),
            )
            .expect("query first student id");

        conn.execute(
            "UPDATE students
             SET points = COALESCE(points, 0) + ?1
             WHERE id = ?2 AND classroom_id = ?3",
            params![1_i64, first_student_id, class_id],
        )
        .expect("apply single student +1");

        conn.execute(
            "UPDATE students
             SET points = COALESCE(points, 0) + ?1
             WHERE classroom_id = ?2",
            params![-1_i64, class_id],
        )
        .expect("apply classroom -1");

        conn.execute(
            "UPDATE students
             SET group_no = ?1
             WHERE classroom_id = ?2 AND seat_no_text IN ('01', '02')",
            params![1_i64, class_id],
        )
        .expect("assign group 1");

        conn.execute(
            "UPDATE students
             SET points = COALESCE(points, 0) + ?1
             WHERE classroom_id = ?2 AND COALESCE(group_no, 0) = ?3",
            params![2_i64, class_id, 1_i64],
        )
        .expect("apply group +2");

        let grouped_student_points: i64 = conn
            .query_row(
                "SELECT points FROM students WHERE classroom_id = ?1 AND seat_no_text = '02'",
                params![class_id],
                |row| row.get(0),
            )
            .expect("query grouped student points");
        assert_eq!(
            grouped_student_points, 1,
            "group operation should apply cumulative score changes",
        );

        conn.execute(
            "UPDATE students
             SET points = 0
             WHERE classroom_id = ?1",
            params![class_id],
        )
        .expect("reset classroom points");

        let max_abs_points: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(ABS(points)), 0) FROM students WHERE classroom_id = ?1",
                params![class_id],
                |row| row.get(0),
            )
            .expect("verify reset points");
        assert_eq!(max_abs_points, 0, "reset should zero all points");

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn baseline_can_upgrade_with_next_release_student_points_migration() {
        let db_path = make_temp_db_path();
        let conn = Connection::open(&db_path).expect("open sqlite database");
        let (_, baseline_sql) = baseline_migration().expect("read baseline migration");
        conn.execute_batch(baseline_sql)
            .expect("apply baseline schema");

        conn.execute_batch(include_str!("../sql/migrations/next_release.sql"))
            .expect("apply next_release draft migration");

        let has_group_no: i64 = conn
            .query_row(
                "SELECT COUNT(1)
                 FROM pragma_table_info('students')
                 WHERE name = 'group_no'",
                [],
                |row| row.get(0),
            )
            .expect("check group_no column");
        let has_points: i64 = conn
            .query_row(
                "SELECT COUNT(1)
                 FROM pragma_table_info('students')
                 WHERE name = 'points'",
                [],
                |row| row.get(0),
            )
            .expect("check points column");

        assert_eq!(has_group_no, 1, "students should have group_no column");
        assert_eq!(has_points, 1, "students should have points column");

        let _ = fs::remove_file(db_path);
    }

    #[test]
    fn line_settings_migration_adds_columns() {
        let db_path = make_temp_db_path();
        let conn = Connection::open(&db_path).expect("open sqlite database");
        let (_, baseline_sql) = baseline_migration().expect("read baseline migration");
        conn.execute_batch(baseline_sql).expect("apply baseline");

        conn.execute_batch(include_str!("../sql/migrations/003_add_line_settings.sql"))
            .expect("apply line settings migration");

        for col in &[
            "line_enabled",
            "line_channel_access_token",
            "line_channel_secret",
            "line_rich_menu_id",
        ] {
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(1) FROM pragma_table_info('classrooms') WHERE name = ?1",
                    params![col],
                    |row| row.get(0),
                )
                .expect("query column existence");
            assert_eq!(count, 1, "classrooms should have column {col}");
        }

        conn.execute(
            "INSERT INTO classrooms (name) VALUES (?1)",
            params!["測試班級"],
        )
        .expect("insert classroom");
        let row: (i64, String, String, String) = conn
            .query_row(
                "SELECT line_enabled, line_channel_access_token, line_channel_secret, \
                 line_rich_menu_id FROM classrooms WHERE name = ?1",
                params!["測試班級"],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )
            .expect("query line defaults");
        assert_eq!(row.0, 0, "line_enabled should default to 0");
        assert_eq!(row.1, "", "line_channel_access_token should default to empty");
        assert_eq!(row.2, "", "line_channel_secret should default to empty");
        assert_eq!(row.3, "", "line_rich_menu_id should default to empty");

        let _ = fs::remove_file(db_path);
    }

    fn make_items(titles: &[&str], date: &str) -> Vec<SyncTaskItem> {
        titles
            .iter()
            .map(|t| SyncTaskItem {
                title: t.to_string(),
                task_date: date.to_string(),
            })
            .collect()
    }

    #[test]
    fn build_sync_text_generates_tasks_and_truncates() {
        let tasks = make_items(
            &["任務編號 1 的內容", "任務編號 2 的內容", "任務編號 3 的內容", "任務編號 4 的內容", "任務編號 5 的內容"],
            "2026-07-09",
        );

        let (text, summarized) = build_sync_text(&tasks);
        assert!(!summarized, "5 short tasks should not require summarization");
        assert!(
            text.contains("=== 聯絡簿"),
            "should have header with date"
        );
        assert!(text.contains("7/9 (四)"), "should show date with weekday");
        assert!(text.contains("任務編號 1"), "should contain first task");
        assert!(text.contains("任務編號 5"), "should contain last task");

        let long_items: Vec<SyncTaskItem> = (1..=20)
            .map(|i| SyncTaskItem {
                title: format!(
                    "這是一個非常長的任務敘述，包含了很多詳細的說明文字，編號是 {}，\
                     目的是要測試當聯絡簿內容過多時，系統能否正確偵測到長度超過 LINE \
                     API 允許的 300 字上限，並自動切換為摘要模式",
                    i
                ),
                task_date: "2026-07-09".to_string(),
            })
            .collect();

        let (short_text, was_summarized) = build_sync_text(&long_items);
        assert!(
            was_summarized,
            "20 long tasks should trigger summarization"
        );
        assert!(
            short_text.len() <= LINE_RICHMENU_MAX_ACTION_TEXT,
            "summarized text should not exceed max length"
        );
    }

    #[test]
    fn build_sync_text_handles_empty_input() {
        let empty: Vec<SyncTaskItem> = vec![];
        let (text, summarized) = build_sync_text(&empty);
        assert!(!summarized);
        assert_eq!(text, "=== 聯絡簿 ===\n");
    }

    #[test]
    fn build_sync_text_shows_date_in_header_when_same_date() {
        let tasks = make_items(
            &["交回條", "帶餐具"],
            "2026-07-10",
        );
        let (text, _) = build_sync_text(&tasks);
        assert!(text.contains("=== 聯絡簿 (7/10 (五)) ==="));
        assert!(text.contains("1. 交回條"));
        assert!(text.contains("2. 帶餐具"));
    }

    #[test]
    fn build_sync_text_groups_by_date_when_different_dates() {
        let tasks = vec![
            SyncTaskItem { title: "交回條".into(), task_date: "2026-07-09".into() },
            SyncTaskItem { title: "帶餐具".into(), task_date: "2026-07-09".into() },
            SyncTaskItem { title: "買文具".into(), task_date: "2026-07-10".into() },
        ];
        let (text, _) = build_sync_text(&tasks);
        assert!(text.contains("=== 聯絡簿 ==="), "no date in header when dates differ");
        assert!(text.contains("7/9 (四)"), "should have first date group");
        assert!(text.contains("7/10 (五)"), "should have second date group");
        assert!(text.contains("1. 交回條"));
        assert!(text.contains("2. 帶餐具"));
        assert!(text.contains("3. 買文具"));
    }

    #[test]
    fn mask_sensitive_and_is_masked_work_correctly() {
        assert!(is_masked("abcd****"), "masked value should be detected");
        assert!(!is_masked(""), "empty string is not masked");
        assert!(!is_masked("realtoken123"), "real value without ****");

        let result = serde_json::to_string(&ClassroomSummary {
            id: 1,
            name: "test".into(),
            line_enabled: true,
            line_channel_access_token: "my-secret-token-here".into(),
            line_channel_secret: "my-secret".into(),
            line_rich_menu_id: "rich123".into(),
        })
        .expect("serialize classroom summary");

        assert!(
            result.contains("my-s****"),
            "token should be masked: {result}"
        );
        assert!(
            result.contains("my-s****"),
            "secret should be masked: {result}"
        );
        assert!(!result.contains("my-secret-token-here"), "full token hidden");
        assert!(!result.contains("my-secret"), "full secret hidden");
    }

    #[test]
    fn init_database_applies_line_settings_migration() {
        let db_path = make_temp_db_path();
        init_database(&db_path).expect("init should apply all migrations");

        let conn = Connection::open(&db_path).expect("open sqlite database");
        let col_count: i64 = conn
            .query_row(
                "SELECT COUNT(1) FROM pragma_table_info('classrooms') \
                 WHERE name IN ('line_enabled', 'line_channel_access_token', \
                                'line_channel_secret', 'line_rich_menu_id')",
                [],
                |row| row.get(0),
            )
            .expect("query line columns");
        assert_eq!(col_count, 4, "all 4 LINE columns should exist");

        let _ = fs::remove_file(db_path);
    }
}
