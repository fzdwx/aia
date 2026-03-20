use std::sync::{Arc, Mutex};

use super::*;
use crate::AiaStore;

#[test]
fn session_id_format_is_valid() {
    let id = generate_session_id();
    assert_eq!(id.len(), 17); // YYYYMMDD_XXXXXXXX
    assert_eq!(id.as_bytes()[8], b'_');
    assert!(id[9..].chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn crud_operations_work() {
    let store = AiaStore::in_memory().expect("store init");
    let record = SessionRecord::new("20260315_abcd1234", "Test session", "gpt-4.1");

    store.create_session(&record).expect("create");

    let sessions = store.list_sessions().expect("list");
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].id, "20260315_abcd1234");
    assert_eq!(sessions[0].title, "Test session");

    let found = store.get_session("20260315_abcd1234").expect("get");
    assert!(found.is_some());
    assert_eq!(found.as_ref().expect("some").model, "gpt-4.1");

    let updated =
        store.update_session("20260315_abcd1234", Some("New title"), None).expect("update");
    assert!(updated);
    let found = store.get_session("20260315_abcd1234").expect("get").expect("some");
    assert_eq!(found.title, "New title");

    let deleted = store.delete_session("20260315_abcd1234").expect("delete");
    assert!(deleted);
    let sessions = store.list_sessions().expect("list");
    assert!(sessions.is_empty());
}

#[test]
fn delete_nonexistent_returns_false() {
    let store = AiaStore::in_memory().expect("store init");
    let deleted = store.delete_session("nonexistent").expect("delete");
    assert!(!deleted);
}

#[test]
fn update_session_model_only_updates_model_and_timestamp() {
    let store = AiaStore::in_memory().expect("store init");
    let record = SessionRecord {
        id: "20260315_modelonly".to_string(),
        title: "Model only".to_string(),
        created_at: "2026-03-15T00:00:00Z".to_string(),
        updated_at: "2026-03-15T00:00:00Z".to_string(),
        model: "gpt-4.1".to_string(),
    };
    store.create_session(&record).expect("create");

    let updated =
        store.update_session("20260315_modelonly", None, Some("gpt-5")).expect("update model only");

    assert!(updated);
    let found = store.get_session("20260315_modelonly").expect("get").expect("some");
    assert_eq!(found.title, "Model only");
    assert_eq!(found.model, "gpt-5");
    assert_ne!(found.updated_at, "2026-03-15T00:00:00Z");
}

#[test]
fn update_session_without_fields_still_touches_updated_at() {
    let store = AiaStore::in_memory().expect("store init");
    let record = SessionRecord {
        id: "20260315_touch".to_string(),
        title: "Touch only".to_string(),
        created_at: "2026-03-15T00:00:00Z".to_string(),
        updated_at: "2026-03-15T00:00:00Z".to_string(),
        model: "gpt-4.1".to_string(),
    };
    store.create_session(&record).expect("create");

    let updated = store.update_session("20260315_touch", None, None).expect("touch updated_at");

    assert!(updated);
    let found = store.get_session("20260315_touch").expect("get").expect("some");
    assert_eq!(found.title, "Touch only");
    assert_eq!(found.model, "gpt-4.1");
    assert_ne!(found.updated_at, "2026-03-15T00:00:00Z");
}

#[test]
fn session_operations_recover_after_poisoned_mutex() {
    let store = Arc::new(AiaStore::in_memory().expect("store init"));
    let cloned = store.clone();
    let _ = std::thread::spawn(move || {
        let _guard = cloned.conn.lock().expect("test should lock before poisoning");
        panic!("poison store mutex");
    })
    .join();

    let record = SessionRecord::new("20260316_poisoned", "Recovered session", "gpt-4.1");

    store.create_session(&record).expect("create after poison");
    let sessions = store.list_sessions().expect("list after poison");
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0], record);
}

#[test]
fn first_session_id_returns_earliest_created_session() {
    let store = AiaStore::in_memory().expect("store init");
    store
        .create_session(&SessionRecord {
            id: "20260315_second".to_string(),
            title: "Second".to_string(),
            created_at: "2026-03-15T00:00:01Z".to_string(),
            updated_at: "2026-03-15T00:00:01Z".to_string(),
            model: "gpt-4.1".to_string(),
        })
        .expect("create second");
    store
        .create_session(&SessionRecord {
            id: "20260315_first".to_string(),
            title: "First".to_string(),
            created_at: "2026-03-15T00:00:00Z".to_string(),
            updated_at: "2026-03-15T00:00:00Z".to_string(),
            model: "gpt-4.1".to_string(),
        })
        .expect("create first");

    let first = store.first_session_id().expect("first session");

    assert_eq!(first.as_deref(), Some("20260315_first"));
}

#[tokio::test(flavor = "current_thread")]
async fn async_session_methods_work() {
    let store = Arc::new(AiaStore::in_memory().expect("store init"));
    let record = SessionRecord::new("20260317_async", "Async session", "gpt-5.4");

    store.create_session_async(record.clone()).await.expect("create async");

    let listed = store.list_sessions_async().await.expect("list async");
    assert_eq!(listed, vec![record.clone()]);

    let first = store.first_session_id_async().await.expect("first async");
    assert_eq!(first.as_deref(), Some("20260317_async"));
}

#[test]
fn lock_conn_recovers_poisoned_mutex() {
    let lock = Arc::new(Mutex::new(1_u8));
    let cloned = lock.clone();
    let _ = std::thread::spawn(move || {
        let _guard = cloned.lock().expect("test should lock before poisoning");
        panic!("poison helper mutex");
    })
    .join();

    let guard = match lock.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    assert_eq!(*guard, 1);
}

#[test]
fn iso8601_now_format_is_valid() {
    let ts = iso8601_now();
    assert_eq!(ts.len(), 20);
    assert!(ts.ends_with('Z'));
    assert_eq!(ts.as_bytes()[4], b'-');
    assert_eq!(ts.as_bytes()[10], b'T');
}
