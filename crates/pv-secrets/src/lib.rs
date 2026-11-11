//! The Anthropic API key, kept in the operating system's credential store (Windows Credential
//! Manager, macOS Keychain).
//!
//! The key goes in once and never comes out: nothing here hands it to the front end. The
//! front end only gets an [`ApiKeyStatus`] (configured, source, last four characters). While
//! in use the key lives in an [`ApiKey`], whose memory is wiped on drop and which never prints
//! itself.
//!
//! Precedence: the `ANTHROPIC_API_KEY` environment variable (development, tests, IT-managed
//! machines), else the credential store. There is no plain-text fallback: when the store is
//! unavailable, saving fails with a translatable error.

use std::fmt;

use pv_model::command::keys;
use pv_model::CommandError;
use serde::{Deserialize, Serialize};
use specta::Type;
use zeroize::Zeroizing;

/// Credential store entry: service and account names.
pub const SERVICE: &str = "ch.younergy.pvstudio";
pub const ACCOUNT: &str = "anthropic-api-key";
/// Environment variable that overrides the stored key.
pub const ENV_VAR: &str = "ANTHROPIC_API_KEY";

/// An API key in memory: wiped on drop, redacted in `Debug` / `Display`.
pub struct ApiKey(Zeroizing<String>);

impl ApiKey {
    /// Checks the shape of a key typed by the user (`sk-ant-…`, no whitespace).
    ///
    /// # Errors
    /// [`keys::SECRETS_FORMAT`].
    pub fn parse(raw: &str) -> Result<Self, CommandError> {
        let key = raw.trim();
        let plausible = key.starts_with("sk-ant-")
            && key.len() >= 20
            && key.len() <= 512
            && !key.chars().any(char::is_whitespace);
        if plausible {
            Ok(Self(Zeroizing::new(key.to_owned())))
        } else {
            Err(CommandError::new(keys::SECRETS_FORMAT))
        }
    }

    /// The key, for the `x-api-key` header only.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// Last four characters, shown to the user to recognise the key.
    #[must_use]
    pub fn hint(&self) -> String {
        let chars: Vec<char> = self.0.chars().collect();
        chars[chars.len().saturating_sub(4)..].iter().collect()
    }
}

impl fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ApiKey(***)")
    }
}

impl fmt::Display for ApiKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("***")
    }
}

/// Where the key in use comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum KeySource {
    Environment,
    CredentialStore,
}

/// What the front end may know about the key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ApiKeyStatus {
    pub configured: bool,
    #[specta(optional)]
    pub source: Option<KeySource>,
    /// Last four characters of the key in use.
    #[specta(optional)]
    pub hint: Option<String>,
    /// The credential store works on this machine (saving is possible).
    pub store_available: bool,
}

/// A place where the key is kept.
pub trait SecretStore {
    /// # Errors
    /// [`keys::SECRETS_UNAVAILABLE`] when the store cannot be read.
    fn load(&self) -> Result<Option<ApiKey>, CommandError>;
    /// # Errors
    /// [`keys::SECRETS_UNAVAILABLE`].
    fn save(&self, key: &ApiKey) -> Result<(), CommandError>;
    /// Deleting a missing entry is not an error.
    ///
    /// # Errors
    /// [`keys::SECRETS_UNAVAILABLE`].
    fn delete(&self) -> Result<(), CommandError>;
    fn available(&self) -> bool;
}

fn unavailable(detail: impl fmt::Display) -> CommandError {
    CommandError::new(keys::SECRETS_UNAVAILABLE).with_text("detail", detail.to_string())
}

/// The operating system's credential store, entry `service` / [`ACCOUNT`].
#[derive(Debug, Clone, Copy)]
pub struct OsStore {
    service: &'static str,
}

impl Default for OsStore {
    fn default() -> Self {
        Self { service: SERVICE }
    }
}

impl OsStore {
    /// Another entry (tests use their own, never the application's).
    #[must_use]
    pub const fn with_service(service: &'static str) -> Self {
        Self { service }
    }
}

#[cfg(any(windows, target_os = "macos"))]
impl OsStore {
    fn entry(&self) -> Result<keyring::Entry, CommandError> {
        keyring::Entry::new(self.service, ACCOUNT).map_err(unavailable)
    }
}

#[cfg(any(windows, target_os = "macos"))]
impl SecretStore for OsStore {
    fn load(&self) -> Result<Option<ApiKey>, CommandError> {
        match self.entry()?.get_password() {
            // A stored value that no longer parses is treated as absent.
            Ok(secret) => Ok(ApiKey::parse(&Zeroizing::new(secret)).ok()),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(unavailable(e)),
        }
    }

    fn save(&self, key: &ApiKey) -> Result<(), CommandError> {
        self.entry()?
            .set_password(key.expose())
            .map_err(unavailable)
    }

    fn delete(&self) -> Result<(), CommandError> {
        match self.entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(unavailable(e)),
        }
    }

    fn available(&self) -> bool {
        self.entry().is_ok()
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
impl SecretStore for OsStore {
    fn load(&self) -> Result<Option<ApiKey>, CommandError> {
        Ok(None)
    }
    fn save(&self, _key: &ApiKey) -> Result<(), CommandError> {
        Err(unavailable(
            "no supported credential store on this platform",
        ))
    }
    fn delete(&self) -> Result<(), CommandError> {
        Ok(())
    }
    fn available(&self) -> bool {
        false
    }
}

/// An in-memory store (tests).
#[derive(Default)]
pub struct MemoryStore(std::sync::Mutex<Option<Zeroizing<String>>>);

impl SecretStore for MemoryStore {
    fn load(&self) -> Result<Option<ApiKey>, CommandError> {
        let guard = self.0.lock().map_err(unavailable)?;
        Ok(guard.as_ref().and_then(|k| ApiKey::parse(k).ok()))
    }
    fn save(&self, key: &ApiKey) -> Result<(), CommandError> {
        *self.0.lock().map_err(unavailable)? = Some(Zeroizing::new(key.expose().to_owned()));
        Ok(())
    }
    fn delete(&self) -> Result<(), CommandError> {
        *self.0.lock().map_err(unavailable)? = None;
        Ok(())
    }
    fn available(&self) -> bool {
        true
    }
}

/// The key in use and its source: the environment variable (`env`, when set and non-empty)
/// wins over the store.
///
/// # Errors
/// [`keys::SECRETS_UNAVAILABLE`] when the store cannot be read.
pub fn resolve(
    env: Option<&str>,
    store: &dyn SecretStore,
) -> Result<Option<(ApiKey, KeySource)>, CommandError> {
    if let Some(v) = env.map(str::trim).filter(|v| !v.is_empty()) {
        // Taken as given: IT may use keys of another shape (proxies).
        return Ok(Some((
            ApiKey(Zeroizing::new(v.to_owned())),
            KeySource::Environment,
        )));
    }
    Ok(store.load()?.map(|k| (k, KeySource::CredentialStore)))
}

/// [`resolve`] with the process environment.
///
/// # Errors
/// As [`resolve`].
pub fn resolve_from_env(
    store: &dyn SecretStore,
) -> Result<Option<(ApiKey, KeySource)>, CommandError> {
    let env = std::env::var(ENV_VAR).ok().map(Zeroizing::new);
    resolve(env.as_deref().map(String::as_str), store)
}

/// The status shown in the settings (never the key).
///
/// # Errors
/// As [`resolve`].
pub fn status(env: Option<&str>, store: &dyn SecretStore) -> Result<ApiKeyStatus, CommandError> {
    let resolved = resolve(env, store)?;
    Ok(ApiKeyStatus {
        configured: resolved.is_some(),
        source: resolved.as_ref().map(|(_, s)| *s),
        hint: resolved.as_ref().map(|(k, _)| k.hint()),
        store_available: store.available(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "sk-ant-api03-abcdefghijklmnopqrstuvwxyz-a1b2";

    #[test]
    fn keys_never_print() {
        let k = ApiKey::parse(KEY).expect("key");
        assert_eq!(format!("{k:?}"), "ApiKey(***)");
        assert_eq!(format!("{k}"), "***");
        assert_eq!(k.hint(), "a1b2");
        let status = ApiKeyStatus {
            configured: true,
            source: Some(KeySource::CredentialStore),
            hint: Some(k.hint()),
            store_available: true,
        };
        assert!(!format!("{status:?}").contains("sk-ant"));
    }

    #[test]
    fn typed_keys_are_checked() {
        assert!(ApiKey::parse(&format!("  {KEY}\n")).is_ok(), "trimmed");
        for bad in [
            "",
            "sk-ant-",
            "sk-proj-abcdefghijklmnopqrstuvwxyz",
            "sk-ant-api03 abc def ghi jkl mno",
        ] {
            assert_eq!(ApiKey::parse(bad).expect_err(bad).key, keys::SECRETS_FORMAT);
        }
    }

    #[test]
    fn environment_wins_over_the_store() {
        let store = MemoryStore::default();
        assert!(!status(None, &store).expect("status").configured);
        store.save(&ApiKey::parse(KEY).expect("key")).expect("save");
        let s = status(None, &store).expect("status");
        assert_eq!(
            (s.configured, s.source, s.hint.as_deref()),
            (true, Some(KeySource::CredentialStore), Some("a1b2"))
        );
        let s = status(Some("sk-ant-from-environment-zz99"), &store).expect("status");
        assert_eq!(
            (s.source, s.hint.as_deref()),
            (Some(KeySource::Environment), Some("zz99"))
        );
        // An empty variable does not hide the stored key.
        assert_eq!(
            status(Some("  "), &store).expect("status").source,
            Some(KeySource::CredentialStore)
        );
        store.delete().expect("delete");
        store.delete().expect("deleting twice is fine");
        assert!(!status(None, &store).expect("status").configured);
    }
}
