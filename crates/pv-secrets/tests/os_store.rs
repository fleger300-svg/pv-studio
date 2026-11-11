//! The real OS credential store (Windows Credential Manager / macOS Keychain) keeps the key
//! across store instances, i.e. it is not an in-memory mock. Uses a dedicated test entry,
//! removed at the end; the application's entry is never touched.

use pv_secrets::{status, ApiKey, KeySource, OsStore, SecretStore};

#[cfg(any(windows, target_os = "macos"))]
#[test]
fn the_key_persists_in_the_os_store() {
    const TEST_SERVICE: &str = "ch.younergy.pvstudio.test";
    let key = "sk-ant-api03-test-only-not-a-real-key-9z8y";
    OsStore::with_service(TEST_SERVICE)
        .delete()
        .expect("clean start");

    OsStore::with_service(TEST_SERVICE)
        .save(&ApiKey::parse(key).expect("key"))
        .expect("save");
    // A fresh instance reads it back: persisted by the OS, not held in memory.
    let fresh = OsStore::with_service(TEST_SERVICE);
    assert_eq!(
        fresh.load().expect("load").map(|k| k.hint()),
        Some("9z8y".to_owned())
    );
    let s = status(None, &fresh).expect("status");
    assert_eq!(s.source, Some(KeySource::CredentialStore));
    assert!(s.store_available);

    fresh.delete().expect("delete");
    assert!(OsStore::with_service(TEST_SERVICE)
        .load()
        .expect("load")
        .is_none());
}
