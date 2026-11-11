//! Settings: the Anthropic API key. It goes in once (`set_api_key`) and never comes out: the
//! front end only gets an `ApiKeyStatus`. The key lives in the OS credential store
//! (`pv_secrets`); in memory only while a request is sent.

use pv_copilot::KeyCheck;
use pv_model::command::keys;
use pv_model::CommandError;
use pv_secrets::{ApiKey, ApiKeyStatus, OsStore, SecretStore};
use zeroize::Zeroizing;

/// Base URL of the Messages API (`ANTHROPIC_BASE_URL` overrides it: tests, proxies).
pub(crate) fn api_base() -> String {
    std::env::var("ANTHROPIC_BASE_URL")
        .ok()
        .map(|b| b.trim().trim_end_matches('/').to_owned())
        .filter(|b| !b.is_empty())
        .unwrap_or_else(|| "https://api.anthropic.com".to_owned())
}

/// The key in use (environment first, then the credential store).
///
/// # Errors
/// [`keys::COPILOT_NO_KEY`] when there is none; [`keys::SECRETS_UNAVAILABLE`].
pub(crate) fn current_key() -> Result<ApiKey, CommandError> {
    pv_secrets::resolve_from_env(&OsStore::default())?
        .map(|(k, _)| k)
        .ok_or_else(|| CommandError::new(keys::COPILOT_NO_KEY))
}

fn status() -> Result<ApiKeyStatus, CommandError> {
    let env = std::env::var(pv_secrets::ENV_VAR).ok().map(Zeroizing::new);
    pv_secrets::status(env.as_deref().map(String::as_str), &OsStore::default())
}

fn network(e: impl std::fmt::Display) -> CommandError {
    CommandError::new(keys::COPILOT_NETWORK).with_text("detail", e.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn api_key_status() -> Result<ApiKeyStatus, CommandError> {
    status()
}

/// Stores the key typed by the user in the credential store (replacing the previous one).
#[tauri::command]
#[specta::specta]
pub async fn set_api_key(key: String) -> Result<ApiKeyStatus, CommandError> {
    // The IPC string is wiped as soon as it is parsed.
    let raw = Zeroizing::new(key);
    let key = ApiKey::parse(&raw)?;
    OsStore::default().save(&key)?;
    status()
}

#[tauri::command]
#[specta::specta]
pub async fn delete_api_key() -> Result<ApiKeyStatus, CommandError> {
    OsStore::default().delete()?;
    status()
}

/// Checks the key in use against the API (lists models: free).
#[tauri::command]
#[specta::specta]
pub async fn test_api_key() -> Result<KeyCheck, CommandError> {
    let key = current_key()?;
    let (path, headers) = pv_copilot::key_check_request();
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(network)?;
    let mut request = client
        .get(format!("{}{path}", api_base()))
        .header("x-api-key", key.expose());
    for (name, value) in headers {
        request = request.header(name, value);
    }
    let response = request.send().await.map_err(network)?;
    Ok(pv_copilot::key_check_outcome(response.status().as_u16()))
}
