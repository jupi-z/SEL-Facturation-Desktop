use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;
use tauri::{Manager, State};

const DEFAULT_CENTRAL_API_URL: &str = "https://sel.shuleops.com";

struct DesktopState {
    central_url: Option<String>,
    data_dir: PathBuf,
    central_client: reqwest::Client,
    central_session: tokio::sync::Mutex<Option<CentralSession>>,
}

fn session_path(data_dir: &Path) -> PathBuf {
    data_dir.join("central-session.dpapi")
}

#[cfg(windows)]
fn protect_session_token(value: &str) -> Result<Vec<u8>, String> {
    use std::ffi::c_void;
    #[repr(C)]
    struct DataBlob {
        size: u32,
        data: *mut u8,
    }
    #[link(name = "Crypt32")]
    unsafe extern "system" {
        fn CryptProtectData(
            input: *const DataBlob,
            description: *const u16,
            entropy: *const DataBlob,
            reserved: *mut c_void,
            prompt: *const c_void,
            flags: u32,
            output: *mut DataBlob,
        ) -> i32;
    }
    #[link(name = "Kernel32")]
    unsafe extern "system" {
        fn LocalFree(memory: *mut c_void) -> *mut c_void;
    }
    let input = DataBlob {
        size: value.len() as u32,
        data: value.as_ptr() as *mut u8,
    };
    let mut output = DataBlob {
        size: 0,
        data: std::ptr::null_mut(),
    };
    let ok = unsafe {
        CryptProtectData(
            &input,
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null_mut(),
            std::ptr::null(),
            1,
            &mut output,
        )
    };
    if ok == 0 {
        return Err("Windows n’a pas pu protéger la session utilisateur.".into());
    }
    let encrypted =
        unsafe { std::slice::from_raw_parts(output.data, output.size as usize).to_vec() };
    unsafe {
        LocalFree(output.data.cast());
    }
    Ok(encrypted)
}

#[cfg(windows)]
fn unprotect_session_token(encrypted: &[u8]) -> Result<String, String> {
    use std::ffi::c_void;
    #[repr(C)]
    struct DataBlob {
        size: u32,
        data: *mut u8,
    }
    #[link(name = "Crypt32")]
    unsafe extern "system" {
        fn CryptUnprotectData(
            input: *const DataBlob,
            description: *mut *mut u16,
            entropy: *const DataBlob,
            reserved: *mut c_void,
            prompt: *const c_void,
            flags: u32,
            output: *mut DataBlob,
        ) -> i32;
    }
    #[link(name = "Kernel32")]
    unsafe extern "system" {
        fn LocalFree(memory: *mut c_void) -> *mut c_void;
    }
    let input = DataBlob {
        size: encrypted.len() as u32,
        data: encrypted.as_ptr() as *mut u8,
    };
    let mut output = DataBlob {
        size: 0,
        data: std::ptr::null_mut(),
    };
    let ok = unsafe {
        CryptUnprotectData(
            &input,
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null_mut(),
            std::ptr::null(),
            1,
            &mut output,
        )
    };
    if ok == 0 {
        return Err("La session Windows protégée est illisible.".into());
    }
    let bytes = unsafe { std::slice::from_raw_parts(output.data, output.size as usize).to_vec() };
    unsafe {
        LocalFree(output.data.cast());
    }
    String::from_utf8(bytes).map_err(|_| "La session Windows protégée est invalide.".into())
}

#[cfg(not(windows))]
fn protect_session_token(_value: &str) -> Result<Vec<u8>, String> {
    Err("La conservation sécurisée de session est disponible uniquement sous Windows.".into())
}
#[cfg(not(windows))]
fn unprotect_session_token(_encrypted: &[u8]) -> Result<String, String> {
    Err("La conservation sécurisée de session est disponible uniquement sous Windows.".into())
}

fn persist_refresh_token(data_dir: &Path, token: &str) -> Result<(), String> {
    let encrypted = protect_session_token(token)?;
    write_private_replace(&session_path(data_dir), &encrypted)
}

fn write_private_replace(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let temporary = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| format!("création de session sécurisée: {error}"))?;
        file.write_all(bytes)
            .and_then(|_| file.sync_all())
            .map_err(|error| format!("écriture de session sécurisée: {error}"))?;
        std::fs::rename(&temporary, path)
            .map_err(|error| format!("remplacement de session sécurisé: {error}"))
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temporary);
    }
    result
}

#[derive(Clone)]
struct CentralSession {
    access_token: String,
    refresh_token: String,
    user_id: String,
    organization_id: String,
    role: String,
    require_password_change: bool,
}

#[derive(Deserialize)]
struct CentralSessionTokens {
    access_token: String,
    refresh_token: String,
    user_id: String,
    organization_id: String,
    role: String,
    #[serde(default)]
    require_password_change: bool,
}

#[derive(Serialize)]
struct CentralResponse {
    status: u16,
    body: Value,
    next_cursor: Option<String>,
}

fn resolve_central_url(
    configured: Option<&str>,
    environment: Option<&str>,
) -> Result<String, String> {
    let endpoint = environment
        .or(configured)
        .unwrap_or(DEFAULT_CENTRAL_API_URL)
        .trim();
    if !endpoint.starts_with("https://") || endpoint.contains('\r') || endpoint.contains('\n') {
        return Err("Le serveur SEL doit être configuré en HTTPS.".into());
    }
    Ok(endpoint.trim_end_matches('/').to_owned())
}
fn validate_central_admin_request(path: &str, method: &str) -> Result<(), String> {
    if path
        .chars()
        .any(|character| matches!(character, '?' | '#' | '%' | '\\' | '\r' | '\n'))
        || path.split('/').any(|part| part == ".." || part == ".")
    {
        return Err("Cette route centrale n’est pas autorisée depuis le client.".into());
    }
    let parts: Vec<&str> = path.split('/').collect();
    if path == "/v1/auth/change-password" && method == "POST" {
        return Ok(());
    }
    if path == "/v1/admin/configuration/readiness" {
        return if method == "GET" {
            Ok(())
        } else {
            Err("Méthode interdite pour cette route.".into())
        };
    }
    if let ["", "v1", "users", id, "password"] = parts.as_slice() {
        if is_uuid_segment(id) && method == "POST" {
            return Ok(());
        }
        return Err("Méthode ou identifiant non autorisé pour cette route.".into());
    }
    if !path.starts_with("/v1/admin/") {
        return Err("Cette route centrale n’est pas autorisée depuis le client.".into());
    }
    let collection = match parts.as_slice() {
        ["", "v1", "admin", collection] => Some(*collection),
        ["", "v1", "admin", "catalog", collection] => Some(*collection),
        ["", "v1", "admin", "users", id] if is_uuid_segment(id) => {
            return if method == "PATCH" {
                Ok(())
            } else {
                Err("Méthode interdite pour cette route.".into())
            };
        }
        ["", "v1", "admin", "catalog", collection, id, action]
            if is_uuid_segment(id) && matches!(*action, "approve" | "revoke") =>
        {
            let action_allowed = match *collection {
                "communication-charge" => *action == "approve",
                "tariffs" | "products" | "tax-profiles" | "fx-rates" => true,
                _ => false,
            };
            if !action_allowed {
                return Err("Référentiel central non autorisé.".into());
            }
            return if method == "POST" {
                Ok(())
            } else {
                Err("Méthode interdite pour cette route.".into())
            };
        }
        _ => None,
    };
    let Some(collection) = collection else {
        return Err("Cette route centrale n’est pas autorisée depuis le client.".into());
    };
    if !matches!(
        collection,
        "users"
            | "audit-events"
            | "tariffs"
            | "products"
            | "tax-profiles"
            | "fx-rates"
            | "communication-charge"
    ) {
        return Err("Référentiel central non autorisé.".into());
    }
    let create_allowed = matches!(
        collection,
        "users" | "tariffs" | "products" | "tax-profiles" | "fx-rates" | "communication-charge"
    );
    match method {
        "GET" => Ok(()),
        "POST" if create_allowed => Ok(()),
        _ => Err("Méthode interdite pour cette route.".into()),
    }
}

fn validate_central_client_request(path_and_query: &str, method: &str) -> Result<(), String> {
    let (path, query) = path_and_query
        .split_once('?')
        .unwrap_or((path_and_query, ""));
    if path.starts_with("/v1/admin/")
        || matches!(path, "/v1/auth/change-password")
        || path.starts_with("/v1/users/")
    {
        return validate_central_admin_request(path, method);
    }
    if path
        .chars()
        .any(|c| matches!(c, '#' | '%' | '\\' | '\r' | '\n'))
        || path.split('/').any(|part| matches!(part, "." | ".."))
        || !valid_central_query(query)
    {
        return Err("Cette route centrale n’est pas autorisée depuis le client.".into());
    }
    let p: Vec<&str> = path.split('/').collect();
    let id = |index: usize| p.get(index).is_some_and(|value| is_uuid_segment(value));
    let allowed = match (method, p.as_slice()) {
        ("GET", ["", "v1", "meta", "capabilities"])
        | ("GET", ["", "v1", "catalog", "products"])
        | ("GET", ["", "v1", "catalog", "tariffs"])
        | ("GET", ["", "v1", "catalog", "tax-profiles"])
        | ("GET", ["", "v1", "catalog", "fx-rates"])
        | ("GET", ["", "v1", "catalog", "snapshot"])
        | ("GET", ["", "v1", "customers"])
        | ("POST", ["", "v1", "customers"])
        | ("GET", ["", "v1", "invoices"])
        | ("GET", ["", "v1", "payments"])
        | ("POST", ["", "v1", "payments"])
        | ("POST", ["", "v1", "billing", "utility", "preview"])
        | ("POST", ["", "v1", "billing", "utility", "issue"])
        | ("POST", ["", "v1", "solar", "quotes"])
        | ("GET" | "POST", ["", "v1", "solar", "sales"])
        | ("POST", ["", "v1", "solar", "contracts"]) => true,
        ("GET", ["", "v1", "customers", _, "statement"]) if id(3) => true,
        ("GET" | "PATCH", ["", "v1", "customers", _]) if id(3) => true,
        ("GET" | "POST", ["", "v1", "customers", _, "service-accounts"]) if id(3) => true,
        ("GET" | "PATCH", ["", "v1", "service-accounts", _]) if id(3) => true,
        ("GET" | "POST", ["", "v1", "service-accounts", _, "meters"]) if id(3) => true,
        ("POST", ["", "v1", "meters", _, "replace"]) if id(3) => true,
        ("GET" | "POST", ["", "v1", "meters", _, "readings"]) if id(3) => true,
        ("POST", ["", "v1", "meter-readings", _, "corrections"]) if id(3) => true,
        ("GET", ["", "v1", "invoices", _]) if id(3) => true,
        ("GET", ["", "v1", "invoices", _, "document-data"]) if id(3) => true,
        ("POST", ["", "v1", "invoices", _, "void"]) if id(3) => true,
        ("GET", ["", "v1", "payments", _, "document-data"]) if id(3) => true,
        ("POST", ["", "v1", "payments", _, "allocate" | "reverse"]) if id(3) => true,
        ("GET", ["", "v1", "receivables", _]) if id(3) => true,
        ("POST", ["", "v1", "receivables", _, "credit"]) if id(3) => true,
        ("GET", ["", "v1", "installment-plans", _]) if id(3) => true,
        ("POST", ["", "v1", "solar", "contracts", _, "activate"]) if id(4) => true,
        ("GET", ["", "v1", "solar", "sales", _, "document-data"]) if id(4) => true,
        _ => false,
    };
    if allowed {
        Ok(())
    } else {
        Err("Cette route ou méthode centrale n’est pas autorisée depuis le client.".into())
    }
}

fn valid_central_query(query: &str) -> bool {
    let bytes = query.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte.is_ascii_alphanumeric() || b"_-.=&+".contains(&byte) {
            index += 1;
            continue;
        }
        if byte != b'%' || index + 2 >= bytes.len() {
            return false;
        }
        let (Some(high), Some(low)) = (hex_value(bytes[index + 1]), hex_value(bytes[index + 2]))
        else {
            return false;
        };
        let decoded = (high << 4) | low;
        if decoded < 0x20 || matches!(decoded, b'#' | b'/' | b'\\') {
            return false;
        }
        index += 3;
    }
    true
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn is_uuid_segment(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
}

#[tauri::command]
async fn central_request(
    state: State<'_, DesktopState>,
    path: String,
    method: String,
    body: Option<Value>,
    if_match: Option<i64>,
    idempotency_key: Option<String>,
) -> Result<CentralResponse, String> {
    central_admin_request(state, path, method, body, if_match, idempotency_key).await
}

#[tauri::command]
async fn central_status(state: State<'_, DesktopState>) -> Result<Value, String> {
    let base = state
        .central_url
        .as_deref()
        .ok_or("Le serveur SEL n’est pas configuré.")?;
    let response = state.central_client.get(format!("{base}/health/ready")).send().await
        .map_err(|_| "Le serveur SEL est inaccessible. Connectez-vous à Internet pour utiliser l’application.".to_owned())?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!(
            "Le serveur SEL est indisponible (HTTP {}).",
            status.as_u16()
        ));
    }
    let body = response.json::<Value>().await.unwrap_or(Value::Null);
    let configured = body
        .get("configured")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    Ok(json!({
        "status": if configured { "CENTRAL_CONNECTED" } else { "CENTRAL_UNCONFIGURED" },
        "configured": configured,
        "pending": 0,
        "conflicts": 0,
        "rejected": 0
    }))
}

fn central_error(status: u16, body: &Value) -> String {
    body.get("error")
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
        .or_else(|| body.get("message").and_then(Value::as_str))
        .map(str::to_owned)
        .unwrap_or_else(|| format!("Le serveur central a répondu HTTP {status}."))
}

async fn central_json_response(response: reqwest::Response) -> Result<CentralResponse, String> {
    let status = response.status().as_u16();
    let next_cursor = response
        .headers()
        .get("x-next-cursor")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let body = if status == 204 {
        Value::Null
    } else {
        response.json::<Value>().await.unwrap_or(Value::Null)
    };
    Ok(CentralResponse {
        status,
        body,
        next_cursor,
    })
}

#[tauri::command]
async fn central_login(
    state: State<'_, DesktopState>,
    identity_ref: String,
    password: String,
) -> Result<Value, String> {
    let base = state
        .central_url
        .as_deref()
        .ok_or("La connexion centrale n’est pas configurée pour ce poste.")?;
    if identity_ref.trim().is_empty() || password.is_empty() {
        return Err("Saisissez votre identifiant et votre mot de passe.".into());
    }
    let response = state
        .central_client
        .post(format!("{base}/v1/auth/login"))
        .json(&json!({"identity_ref": identity_ref, "password": password}))
        .send()
        .await
        .map_err(|error| format!("Service central inaccessible : {error}"))?;
    let status = response.status().as_u16();
    let value = response.json::<Value>().await.unwrap_or(Value::Null);
    if !(200..300).contains(&status) {
        return Err(central_error(status, &value));
    }
    let tokens: CentralSessionTokens = serde_json::from_value(value)
        .map_err(|_| "Réponse de session centrale invalide.".to_owned())?;
    persist_refresh_token(&state.data_dir, &tokens.refresh_token)?;
    let public = json!({"user_id":tokens.user_id,"organization_id":tokens.organization_id,"role":tokens.role,"require_password_change":tokens.require_password_change});
    *state.central_session.lock().await = Some(CentralSession {
        access_token: tokens.access_token,
        refresh_token: tokens.refresh_token,
        user_id: tokens.user_id,
        organization_id: tokens.organization_id,
        role: tokens.role,
        require_password_change: tokens.require_password_change,
    });
    Ok(public)
}

#[tauri::command]
async fn central_session_status(state: State<'_, DesktopState>) -> Result<Value, String> {
    let mut server_reachable = true;
    if state.central_session.lock().await.is_none() {
        let path = session_path(&state.data_dir);
        if path.is_file() {
            let restored = std::fs::read(&path)
                .map_err(|error| format!("lecture de session sécurisée: {error}"))
                .and_then(|bytes| unprotect_session_token(&bytes));
            if let Ok(refresh_token) = &restored {
                let base = state
                    .central_url
                    .as_deref()
                    .ok_or("Le serveur SEL n’est pas configuré.")?;
                let response = state
                    .central_client
                    .post(format!("{base}/v1/auth/refresh"))
                    .json(&json!({"refresh_token":refresh_token}))
                    .send()
                    .await;
                if let Ok(response) = response {
                    let status = response.status().as_u16();
                    let value = response.json::<Value>().await.unwrap_or(Value::Null);
                    if (200..300).contains(&status) {
                        if let Ok(tokens) = serde_json::from_value::<CentralSessionTokens>(value) {
                            persist_refresh_token(&state.data_dir, &tokens.refresh_token)?;
                            *state.central_session.lock().await = Some(CentralSession {
                                access_token: tokens.access_token,
                                refresh_token: tokens.refresh_token,
                                user_id: tokens.user_id,
                                organization_id: tokens.organization_id,
                                role: tokens.role,
                                require_password_change: tokens.require_password_change,
                            });
                        }
                    } else if matches!(status, 401 | 403) {
                        let _ = std::fs::remove_file(&path);
                    }
                } else {
                    server_reachable = false;
                }
            }
            if restored.is_err() {
                let _ = std::fs::remove_file(path);
            }
        }
    }
    let session = state.central_session.lock().await;
    Ok(match session.as_ref() {
        Some(value) => {
            json!({"authenticated":true,"user_id":value.user_id,"organization_id":value.organization_id,"role":value.role,"require_password_change":value.require_password_change})
        }
        None => json!({"authenticated":false,"server_reachable":server_reachable}),
    })
}

struct CentralAdminRequest<'a> {
    path: &'a str,
    method: &'a str,
    token: &'a str,
    body: Option<Value>,
    if_match: Option<i64>,
    idempotency_key: &'a str,
}

async fn send_central_admin(
    client: &reqwest::Client,
    base: &str,
    admin_request: CentralAdminRequest<'_>,
) -> Result<reqwest::Response, String> {
    let idempotent_write = matches!(admin_request.method, "POST" | "PATCH");
    let method = reqwest::Method::from_bytes(admin_request.method.as_bytes())
        .map_err(|_| "Méthode HTTP invalide.".to_owned())?;
    let mut request = client
        .request(method, format!("{base}{}", admin_request.path))
        .bearer_auth(admin_request.token);
    if idempotent_write {
        request = request.header("Idempotency-Key", admin_request.idempotency_key);
    }
    if let Some(revision) = admin_request.if_match {
        request = request.header("If-Match", revision.to_string());
    }
    if let Some(value) = admin_request.body {
        request = request.json(&value);
    }
    request
        .send()
        .await
        .map_err(|error| format!("Service central inaccessible : {error}"))
}

#[tauri::command]
async fn central_admin_request(
    state: State<'_, DesktopState>,
    path: String,
    method: String,
    body: Option<Value>,
    if_match: Option<i64>,
    idempotency_key: Option<String>,
) -> Result<CentralResponse, String> {
    let method = method.to_ascii_uppercase();
    validate_central_client_request(&path, &method)?;
    let base = state
        .central_url
        .as_deref()
        .ok_or("La connexion centrale n’est pas configurée pour ce poste.")?;
    let session = state
        .central_session
        .lock()
        .await
        .as_ref()
        .cloned()
        .ok_or("Connectez-vous avec votre compte SEL pour accéder à l’administration.")?;
    if session.role != "ADMIN" && (path.starts_with("/v1/admin/") || path.starts_with("/v1/users/"))
    {
        return Err("Cette fonction est réservée au rôle administrateur SEL.".into());
    }
    let idempotency_key = idempotency_key.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let response = send_central_admin(
        &state.central_client,
        base,
        CentralAdminRequest {
            path: &path,
            method: &method,
            token: &session.access_token,
            body: body.clone(),
            if_match,
            idempotency_key: &idempotency_key,
        },
    )
    .await?;
    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        let mut session_guard = state.central_session.lock().await;
        let current = session_guard
            .as_mut()
            .ok_or("La session a été fermée. Reconnectez-vous.")?;
        let next_token = if current.access_token != session.access_token {
            current.access_token.clone()
        } else {
            let refreshed = state
                .central_client
                .post(format!("{base}/v1/auth/refresh"))
                .json(&json!({"refresh_token":current.refresh_token}))
                .send()
                .await
                .map_err(|error| format!("Renouvellement de session impossible : {error}"))?;
            let refresh_status = refreshed.status().as_u16();
            let value = refreshed.json::<Value>().await.unwrap_or(Value::Null);
            if !(200..300).contains(&refresh_status) {
                *session_guard = None;
                let _ = std::fs::remove_file(session_path(&state.data_dir));
                return Err("La session a expiré. Reconnectez-vous.".into());
            }
            let tokens: CentralSessionTokens = serde_json::from_value(value)
                .map_err(|_| "Réponse de renouvellement invalide.".to_owned())?;
            let refreshed_session = CentralSession {
                access_token: tokens.access_token,
                refresh_token: tokens.refresh_token,
                user_id: tokens.user_id,
                organization_id: tokens.organization_id,
                role: tokens.role,
                require_password_change: tokens.require_password_change,
            };
            persist_refresh_token(&state.data_dir, &refreshed_session.refresh_token)?;
            let access_token = refreshed_session.access_token.clone();
            *current = refreshed_session;
            access_token
        };
        drop(session_guard);
        let retry = send_central_admin(
            &state.central_client,
            base,
            CentralAdminRequest {
                path: &path,
                method: &method,
                token: &next_token,
                body,
                if_match,
                idempotency_key: &idempotency_key,
            },
        )
        .await?;
        let result = central_json_response(retry).await?;
        if path == "/v1/auth/change-password" && (200..300).contains(&result.status) {
            *state.central_session.lock().await = None;
            let _ = std::fs::remove_file(session_path(&state.data_dir));
        }
        return Ok(result);
    }
    let result = central_json_response(response).await?;
    if path == "/v1/auth/change-password" && (200..300).contains(&result.status) {
        *state.central_session.lock().await = None;
        let _ = std::fs::remove_file(session_path(&state.data_dir));
    }
    Ok(result)
}

#[tauri::command]
async fn central_logout(state: State<'_, DesktopState>) -> Result<(), String> {
    let base = state
        .central_url
        .as_deref()
        .ok_or("La connexion centrale n’est pas configurée pour ce poste.")?;
    let mut session = state.central_session.lock().await;
    let mut remote_error = None;
    if let Some(current) = session.as_ref() {
        match state
            .central_client
            .post(format!("{base}/v1/auth/logout"))
            .bearer_auth(&current.access_token)
            .send()
            .await
        {
            Ok(response) if response.status().is_success() => {}
            Ok(_) => {
                remote_error = Some("Le serveur central n’a pas confirmé la déconnexion.".into())
            }
            Err(error) => {
                remote_error = Some(format!("Déconnexion centrale non confirmée : {error}"))
            }
        }
    }
    *session = None;
    let _ = std::fs::remove_file(session_path(&state.data_dir));
    match remote_error {
        Some(error) => Err(error),
        None => Ok(()),
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
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .setup(|app| {
            let data_dir = app.path().app_local_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let environment_url = std::env::var("SEL_V2_API_URL").ok();
            let central_url = resolve_central_url(None, environment_url.as_deref())
                .map_err(std::io::Error::other)?;
            let central_client = reqwest::Client::builder()
                .timeout(Duration::from_secs(20))
                .build()
                .map_err(std::io::Error::other)?;
            app.manage(DesktopState {
                central_url: Some(central_url),
                data_dir,
                central_client,
                central_session: tokio::sync::Mutex::new(None),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            central_request,
            central_status,
            central_login,
            central_session_status,
            central_admin_request,
            central_logout,
            open_monarque_site
        ])
        .run(tauri::generate_context!())
        .expect("impossible de démarrer SEL Facturation");
}

#[cfg(test)]
mod tests {
    use super::{
        resolve_central_url, validate_central_admin_request, validate_central_client_request,
        CentralResponse, DEFAULT_CENTRAL_API_URL,
    };

    #[test]
    fn central_business_bridge_allows_only_explicit_online_routes() {
        for (path, method) in [
            ("/v1/customers?limit=100", "GET"),
            ("/v1/customers?limit=100&q=Jean%20M%C3%A9ric", "GET"),
            ("/v1/customers", "POST"),
            ("/v1/billing/utility/preview", "POST"),
            ("/v1/billing/utility/issue", "POST"),
            (
                "/v1/invoices/3e5f9d4a-e3a2-4ce2-82f9-f50cc4497aa0/document-data",
                "GET",
            ),
            ("/v1/payments", "POST"),
            ("/v1/solar/sales?limit=100", "GET"),
            (
                "/v1/solar/sales/3e5f9d4a-e3a2-4ce2-82f9-f50cc4497aa0/document-data",
                "GET",
            ),
            ("/v1/solar/quotes", "POST"),
            (
                "/v1/meters/3e5f9d4a-e3a2-4ce2-82f9-f50cc4497aa0/replace",
                "POST",
            ),
            ("/v1/catalog/products", "GET"),
            ("/v1/admin/catalog/tariffs", "GET"),
        ] {
            assert!(
                validate_central_client_request(path, method).is_ok(),
                "{method} {path}"
            );
        }
        for (path, method) in [
            ("/v1/customers", "DELETE"),
            ("/v1/unlisted", "GET"),
            ("https://evil.example/v1/customers", "GET"),
            ("/v1/customers/../../payments", "GET"),
            ("/v1/customers?x=%2F", "GET"),
            ("/v1/customers?q=Jean%0AMarie", "GET"),
            ("/v1/customers?q=%GG", "GET"),
            ("/v1/sync/trigger", "POST"),
            ("/v1/sync/status", "GET"),
            ("/v1/sync/pull", "GET"),
            ("/local/v1/customers", "GET"),
        ] {
            assert!(
                validate_central_client_request(path, method).is_err(),
                "{method} {path}"
            );
        }
    }

    #[test]
    fn central_admin_bridge_accepts_allowlisted_routes() {
        for (path, method) in [
            ("/v1/auth/change-password", "POST"),
            ("/v1/admin/users", "GET"),
            ("/v1/admin/users", "POST"),
            ("/v1/admin/audit-events", "GET"),
            ("/v1/admin/configuration/readiness", "GET"),
            ("/v1/admin/catalog/communication-charge", "GET"),
            ("/v1/admin/catalog/communication-charge", "POST"),
            ("/v1/admin/catalog/tariffs", "GET"),
            ("/v1/admin/catalog/products", "POST"),
            (
                "/v1/admin/users/3e5f9d4a-e3a2-4ce2-82f9-f50cc4497aa0",
                "PATCH",
            ),
            (
                "/v1/users/3e5f9d4a-e3a2-4ce2-82f9-f50cc4497aa0/password",
                "POST",
            ),
            (
                "/v1/admin/catalog/tariffs/3e5f9d4a-e3a2-4ce2-82f9-f50cc4497aa0/approve",
                "POST",
            ),
            (
                "/v1/admin/catalog/communication-charge/3e5f9d4a-e3a2-4ce2-82f9-f50cc4497aa0/approve",
                "POST",
            ),
            (
                "/v1/admin/catalog/products/3e5f9d4a-e3a2-4ce2-82f9-f50cc4497aa0/revoke",
                "POST",
            ),
        ] {
            assert!(
                validate_central_admin_request(path, method).is_ok(),
                "{method} {path}"
            );
        }
        for (path, method) in [
            (
                "/v1/admin/devices/3e5f9d4a-e3a2-4ce2-82f9-f50cc4497aa0/provision",
                "POST",
            ),
            ("/v1/admin/offline-grants", "GET"),
            ("/v1/admin/configuration/readiness", "POST"),
            (
                "/v1/admin/offline-grants/3e5f9d4a-e3a2-4ce2-82f9-f50cc4497aa0/revoke",
                "POST",
            ),
            (
                "/v1/admin/catalog/communication-charge/3e5f9d4a-e3a2-4ce2-82f9-f50cc4497aa0/revoke",
                "POST",
            ),
        ] {
            assert!(
                validate_central_admin_request(path, method).is_err(),
                "{method} {path}"
            );
        }
    }

    #[test]
    fn production_api_url_is_configured_without_local_device_enrollment() {
        assert_eq!(
            resolve_central_url(None, None).unwrap(),
            DEFAULT_CENTRAL_API_URL
        );
        assert_eq!(
            resolve_central_url(Some("https://sel.example/"), None).unwrap(),
            "https://sel.example"
        );
        assert!(resolve_central_url(Some("http://sel.example"), None).is_err());
    }

    #[test]
    fn desktop_response_preserves_customer_pagination_cursor() {
        let response = CentralResponse {
            status: 200,
            body: serde_json::json!([]),
            next_cursor: Some("SEL-C-00100".into()),
        };
        let serialized = serde_json::to_value(response).unwrap();
        assert_eq!(serialized["next_cursor"], "SEL-C-00100");
        assert_eq!(serialized["body"], serde_json::json!([]));
    }

    #[test]
    fn central_admin_bridge_rejects_unlisted_routes_methods_and_traversal() {
        for (path, method) in [
            ("/v1/customers", "GET"),
            ("/v1/admin/users/../../users", "GET"),
            ("/v1/admin/users?organization=other", "GET"),
            (
                "/v1/admin/catalog/products/3e5f9d4a-e3a2-4ce2-82f9-f50cc4497aa0/revoke",
                "DELETE",
            ),
            ("/v1/admin/devices", "GET"),
            ("/v1/admin/devices", "POST"),
            ("/v1/admin/offline-grants", "GET"),
            ("/v1/devices/not-a-uuid/revoke", "POST"),
            ("/v1/admin/audit-events", "POST"),
            ("/v1/admin/audit", "GET"),
            ("/v1/admin/users/not-a-uuid", "PATCH"),
            (
                "/v1/users/3e5f9d4a-e3a2-4ce2-82f9-f50cc4497aa0/password",
                "GET",
            ),
        ] {
            assert!(
                validate_central_admin_request(path, method).is_err(),
                "{method} {path}"
            );
        }
    }
}
