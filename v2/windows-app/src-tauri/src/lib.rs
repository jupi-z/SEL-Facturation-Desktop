use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{Manager, State};

#[derive(Debug, Deserialize)]
struct DeviceConfig {
    organization_id: String,
    device_id: String,
    auth_token: String,
    #[serde(default)]
    actor_id: Option<String>,
    #[serde(default)]
    offline_grant_id: Option<String>,
    #[serde(default)]
    central_api_url: Option<String>,
    #[serde(default)]
    central_api_token: Option<String>,
}

struct LocalAgent {
    child: Mutex<Child>,
    address: SocketAddr,
    token: String,
}

impl Drop for LocalAgent {
    fn drop(&mut self) {
        if let Ok(child) = self.child.get_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

struct DesktopState {
    agent: Option<Arc<LocalAgent>>,
}

#[derive(Serialize)]
struct LocalResponse {
    status: u16,
    body: Value,
}

fn config_path(data_dir: &Path) -> PathBuf {
    std::env::var_os("SEL_V2_DEVICE_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|| data_dir.join("device.json"))
}

fn start_agent(app: &tauri::AppHandle, data_dir: &Path) -> Result<Option<Arc<LocalAgent>>, String> {
    let path = config_path(data_dir);
    if !path.is_file() {
        return Ok(None);
    }
    let config: DeviceConfig = serde_json::from_slice(
        &std::fs::read(&path).map_err(|error| format!("lecture de la configuration du poste: {error}"))?,
    )
    .map_err(|error| format!("configuration du poste invalide: {error}"))?;
    for (name, value) in [
        ("organization_id", config.organization_id.as_str()),
        ("device_id", config.device_id.as_str()),
        ("auth_token", config.auth_token.as_str()),
    ] {
        if value.trim().is_empty() || value.contains('\r') || value.contains('\n') {
            return Err(format!("{name} est absent ou invalide; le poste doit être enrôlé."));
        }
    }
    if config.auth_token.trim().len() < 16 {
        return Err("Le jeton local configuré est trop court.".into());
    }
    if config.actor_id.is_some() != config.offline_grant_id.is_some() {
        return Err("L’identité agent et son autorisation hors ligne doivent être configurées ensemble.".into());
    }
    if config.central_api_url.is_some() != config.central_api_token.is_some() {
        return Err("L’URL centrale et le jeton de synchronisation doivent être configurés ensemble.".into());
    }
    if let Some(endpoint) = config.central_api_url.as_deref() {
        if !endpoint.starts_with("https://") || endpoint.contains('\r') || endpoint.contains('\n') {
            return Err("Le service central doit utiliser une URL HTTPS valide.".into());
        }
    }

    let binary = std::env::var_os("SEL_LOCAL_BINARY")
        .map(PathBuf::from)
        .unwrap_or(app.path().resource_dir().map_err(|error| error.to_string())?.join("sel-local.exe"));
    if !binary.is_file() {
        return Err("L’agent local sel-local.exe est absent du paquet.".into());
    }
    let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(|error| error.to_string())?;
    let address = listener.local_addr().map_err(|error| error.to_string())?;
    drop(listener);

    let database = data_dir.join("sel-local-v2.sqlite");
    let db_url = format!("sqlite:///{}", database.to_string_lossy().replace('\\', "/").trim_start_matches('/'));
    let mut command = Command::new(binary);
    command.env_clear();
    for name in ["SYSTEMROOT", "WINDIR", "TEMP", "TMP", "PATH", "USERPROFILE", "LOCALAPPDATA", "APPDATA"] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    command
        .env("SEL_LOCAL_PORT", address.port().to_string())
        .env("SEL_LOCAL_DATABASE_URL", db_url)
        .env("SEL_ORGANIZATION_ID", config.organization_id)
        .env("SEL_DEVICE_ID", config.device_id)
        .env("SEL_LOCAL_AUTH_TOKEN", &config.auth_token)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let (Some(actor_id), Some(grant_id)) = (config.actor_id, config.offline_grant_id) {
        command.env("SEL_ACTOR_ID", actor_id).env("SEL_OFFLINE_GRANT_ID", grant_id);
    }
    if let (Some(url), Some(token)) = (config.central_api_url, config.central_api_token) {
        command.env("SEL_CENTRAL_API_URL", url).env("SEL_CENTRAL_API_TOKEN", token);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command.spawn().map_err(|error| format!("démarrage de l’agent local: {error}"))?;
    let mut ready = false;
    for _ in 0..50 {
        if TcpStream::connect_timeout(&address, Duration::from_millis(100)).is_ok() {
            ready = true;
            break;
        }
        if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
            return Err(format!("l’agent local s’est arrêté au démarrage ({status})."));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    if !ready {
        let _ = child.kill();
        let _ = child.wait();
        return Err("l’agent local n’a pas ouvert son service dans le délai prévu.".into());
    }
    Ok(Some(Arc::new(LocalAgent { child: Mutex::new(child), address, token: config.auth_token })))
}

fn validate_request(path: &str, method: &str) -> Result<(), String> {
    if !path.starts_with("/local/v1/")
        || path.contains('\r')
        || path.contains('\n')
        || path.contains('\\')
        || path.split('/').any(|part| part == "..")
    {
        return Err("Seules les routes locales SEL V2 sont autorisées.".into());
    }
    if !matches!(method, "GET" | "POST" | "PATCH") {
        return Err("Méthode HTTP non autorisée.".into());
    }
    Ok(())
}

fn request_local(agent: &LocalAgent, path: &str, method: &str, body: Option<Value>, if_match: Option<i64>) -> Result<LocalResponse, String> {
    validate_request(path, method)?;
    if if_match.is_some_and(|revision| revision < 1) {
        return Err("La révision If-Match doit être positive.".into());
    }
    let payload = body.map(|value| serde_json::to_vec(&value)).transpose().map_err(|error| error.to_string())?.unwrap_or_default();
    let mut stream = TcpStream::connect_timeout(&agent.address, Duration::from_secs(3))
        .map_err(|error| format!("service local indisponible: {error}"))?;
    stream.set_read_timeout(Some(Duration::from_secs(15))).map_err(|error| error.to_string())?;
    stream.set_write_timeout(Some(Duration::from_secs(5))).map_err(|error| error.to_string())?;
    let revision_header = if_match.map(|revision| format!("If-Match: {revision}\r\n")).unwrap_or_default();
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer {}\r\n{revision_header}Accept: application/json\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        agent.token, payload.len()
    );
    stream.write_all(request.as_bytes()).and_then(|_| stream.write_all(&payload)).map_err(|error| error.to_string())?;
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).map_err(|error| error.to_string())?;
    let split = raw.windows(4).position(|window| window == b"\r\n\r\n").ok_or("réponse HTTP locale mal formée")?;
    let headers = String::from_utf8_lossy(&raw[..split]);
    let status = headers.lines().next().and_then(|line| line.split_whitespace().nth(1))
        .and_then(|value| value.parse::<u16>().ok()).ok_or("statut HTTP local invalide")?;
    let bytes = &raw[split + 4..];
    let response_body = if headers.to_ascii_lowercase().contains("transfer-encoding: chunked") {
        decode_chunks(bytes)?
    } else {
        bytes.to_vec()
    };
    let body = if response_body.is_empty() { Value::Null } else {
        serde_json::from_slice(&response_body).unwrap_or_else(|_| json!({"message": String::from_utf8_lossy(&response_body)}))
    };
    Ok(LocalResponse { status, body })
}

fn decode_chunks(mut bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut output = Vec::new();
    loop {
        let line_end = bytes.windows(2).position(|window| window == b"\r\n").ok_or("réponse HTTP chunked mal formée")?;
        let length = usize::from_str_radix(std::str::from_utf8(&bytes[..line_end]).map_err(|error| error.to_string())?.trim(), 16)
            .map_err(|error| error.to_string())?;
        bytes = &bytes[line_end + 2..];
        if length == 0 { break; }
        if bytes.len() < length + 2 { return Err("réponse HTTP chunked tronquée".into()); }
        output.extend_from_slice(&bytes[..length]);
        bytes = &bytes[length + 2..];
    }
    Ok(output)
}

#[tauri::command]
async fn local_request(state: State<'_, DesktopState>, path: String, method: String, body: Option<Value>, if_match: Option<i64>) -> Result<LocalResponse, String> {
    let agent = state.agent.as_ref().cloned().ok_or("Ce poste doit être activé et enrôlé avant d’accéder au service local.")?;
    tauri::async_runtime::spawn_blocking(move || request_local(&agent, &path, &method.to_ascii_uppercase(), body, if_match))
        .await
        .map_err(|error| format!("requête locale interrompue: {error}"))?
}

#[tauri::command]
async fn local_status(state: State<'_, DesktopState>) -> Result<Value, String> {
    match state.agent.as_ref().cloned() {
        Some(agent) => tauri::async_runtime::spawn_blocking(move || request_local(&agent, "/local/v1/sync/status", "GET", None, None).map(|response| response.body))
            .await
            .map_err(|error| format!("lecture de l’état local interrompue: {error}"))?,
        None => Ok(json!({"status":"ACTIVATION_REQUIRED","message":"Ce poste doit être activé et enrôlé par un administrateur SEL."})),
    }
}

#[tauri::command]
fn open_monarque_site() -> Result<(), String> {
    #[cfg(windows)]
    {
        Command::new("explorer.exe")
            .arg("https://monarquegrp.com")
            .spawn()
            .map(|_| ())
            .map_err(|error| format!("impossible d’ouvrir le navigateur externe: {error}"))
    }
    #[cfg(not(windows))]
    {
        Err("L’ouverture du site éditeur est disponible depuis l’application Windows.".into())
    }
}

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let data_dir = app.path().app_local_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let agent = start_agent(app.handle(), &data_dir)
                .map_err(|error| std::io::Error::other(error))?;
            app.manage(DesktopState { agent });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![local_request, local_status, open_monarque_site])
        .run(tauri::generate_context!())
        .expect("impossible de démarrer SEL Facturation");
}
