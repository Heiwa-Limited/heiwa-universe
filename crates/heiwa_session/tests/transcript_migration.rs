use heiwa_evidence::{OperatorEventType, OperatorJournal};
use heiwa_protocol::TranscriptBlock;
use heiwa_session::operator::OperatorSessionService;
use heiwa_session::{
    append_entry, get_session_index_path, import_legacy_sessions_with_service, load_transcript,
    save_entries, save_transcript, search_session_messages, set_parent_session_id,
    PersistedTranscript, TranscriptEntry, PERSISTED_TRANSCRIPT_VERSION,
};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use uuid::Uuid;

fn with_temp_home<T>(f: impl FnOnce(&PathBuf) -> T) -> T {
    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();

    let tmp = env::temp_dir().join(format!("heiwa-session-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&tmp).expect("create temp home");

    let original_home = env::var_os("HOME");
    let original_evidence = env::var_os("HEIWA_EVIDENCE_DIR");
    env::set_var("HOME", &tmp);
    // HOME alone does NOT isolate this on Windows. The evidence plane resolves
    // its root through `dirs::home_dir()`, which reads $HOME on Unix but calls
    // the Windows known-folder API and ignores HOME entirely. Without this,
    // every test on Windows resolved to the real user profile, shared one
    // evidence corpus, and leaked state into its siblings - which is why
    // `empty_file_returns_empty_transcript` saw four entries it never wrote.
    // HEIWA_EVIDENCE_DIR is the documented override and short-circuits the
    // home lookup on every platform.
    env::set_var("HEIWA_EVIDENCE_DIR", tmp.join(".heiwa").join("evidence"));

    let result = f(&tmp);

    match original_home {
        Some(v) => env::set_var("HOME", v),
        None => env::remove_var("HOME"),
    }
    match original_evidence {
        Some(v) => env::set_var("HEIWA_EVIDENCE_DIR", v),
        None => env::remove_var("HEIWA_EVIDENCE_DIR"),
    }
    let _ = fs::remove_dir_all(&tmp);

    result
}

fn write_legacy_v0(home: &Path, session_id: &str, body: &str) {
    let sessions = home.join(".heiwa").join("sessions");
    fs::create_dir_all(&sessions).unwrap();
    fs::write(sessions.join(format!("{}.json", session_id)), body).unwrap();
}

#[test]
fn legacy_import_is_idempotent_and_preserves_source_bytes() {
    let source = tempfile::tempdir().unwrap();
    let evidence = tempfile::tempdir().unwrap();
    let body = r#"{
        "session_id": "legacy",
        "transcript": [
            { "User": "hello" },
            { "Assistant": "hi" },
            { "Tool": ["sh", "ok"] },
            { "Evidence": "route ok" }
        ]
    }"#;
    fs::write(source.path().join("legacy.json"), body).unwrap();
    let original = fs::read(source.path().join("legacy.json")).unwrap();
    let service =
        OperatorSessionService::new(OperatorJournal::new(evidence.path().to_path_buf()).unwrap());

    let first = import_legacy_sessions_with_service(&service, source.path()).unwrap();
    let second = import_legacy_sessions_with_service(&service, source.path()).unwrap();

    assert_eq!(first.imported_entries, 4);
    assert_eq!(second.imported_entries, 0);
    assert_eq!(
        fs::read(source.path().join("legacy.json")).unwrap(),
        original
    );
    let events = service.events_after("legacy", None, 100).unwrap().events;
    assert_eq!(
        events
            .iter()
            .filter(|row| row.event.event_type == OperatorEventType::LegacySessionImported)
            .count(),
        1
    );
}

#[test]
fn legacy_import_rejects_sensitive_material_before_appending_a_marker() {
    let source = tempfile::tempdir().unwrap();
    let evidence = tempfile::tempdir().unwrap();
    let body = r#"{"session_id":"legacy-sensitive","transcript":[{"User":"ghp_live-token"}]}"#;
    fs::write(source.path().join("legacy-sensitive.json"), body).unwrap();
    let original = fs::read(source.path().join("legacy-sensitive.json")).unwrap();
    let service =
        OperatorSessionService::new(OperatorJournal::new(evidence.path().to_path_buf()).unwrap());

    let error = import_legacy_sessions_with_service(&service, source.path()).unwrap_err();

    assert!(error.to_string().contains("sensitive"));
    assert_eq!(
        fs::read(source.path().join("legacy-sensitive.json")).unwrap(),
        original
    );
    assert!(service
        .events_after("legacy-sensitive", None, 100)
        .unwrap()
        .events
        .is_empty());
}

#[test]
fn legacy_import_scans_sensitive_ignored_fields_before_marker_lookup() {
    let source = tempfile::tempdir().unwrap();
    let evidence = tempfile::tempdir().unwrap();
    let path = source.path().join("ignored-sensitive.json");
    let body = r#"{"session_id":"ignored-sensitive","transcript":[{"User":"safe"}],"ignored":{"token":"ghp_live-token"}}"#;
    fs::write(&path, body).unwrap();
    let service =
        OperatorSessionService::new(OperatorJournal::new(evidence.path().to_path_buf()).unwrap());
    assert!(import_legacy_sessions_with_service(&service, source.path()).is_err());
    assert_eq!(fs::read(&path).unwrap(), body.as_bytes());
    assert!(service
        .events_after("ignored-sensitive", None, 100)
        .unwrap()
        .events
        .is_empty());
}

#[test]
fn legacy_v1_metadata_round_trips_exactly_through_operator_events() {
    with_temp_home(|home| {
        let body = r#"{"version":1,"session_id":"exact","parent_session_id":"parent","next_entry_id":42,"entries":[{"id":7,"ts_unix_ms":1234,"char_len":5,"block":{"User":"hello"},"embedding_ref":{"model":"m","dim":3,"row_id":9}},{"id":19,"ts_unix_ms":5678,"char_len":2,"block":{"Assistant":"ok"}}]}"#;
        write_legacy_v0(home, "exact", body);
        let transcript = load_transcript("exact").unwrap();
        assert_eq!(transcript.parent_session_id.as_deref(), Some("parent"));
        assert_eq!(transcript.next_entry_id, 42);
        assert_eq!(
            transcript
                .entries
                .iter()
                .map(|entry| entry.id)
                .collect::<Vec<_>>(),
            vec![7, 19]
        );
        assert_eq!(
            transcript
                .entries
                .iter()
                .map(|entry| entry.ts_unix_ms)
                .collect::<Vec<_>>(),
            vec![1234, 5678]
        );
        assert_eq!(
            transcript.entries[0].embedding_ref.as_ref().unwrap().row_id,
            9
        );
    });
}

#[test]
fn changed_legacy_source_with_same_deterministic_ids_is_rejected() {
    let source = tempfile::tempdir().unwrap();
    let evidence = tempfile::tempdir().unwrap();
    let path = source.path().join("conflict.json");
    fs::write(
        &path,
        r#"{"session_id":"conflict","transcript":[{"User":"first"}]}"#,
    )
    .unwrap();
    let service =
        OperatorSessionService::new(OperatorJournal::new(evidence.path().to_path_buf()).unwrap());
    import_legacy_sessions_with_service(&service, source.path()).unwrap();
    fs::write(
        &path,
        r#"{"session_id":"conflict","transcript":[{"User":"changed"}]}"#,
    )
    .unwrap();
    let error = import_legacy_sessions_with_service(&service, source.path()).unwrap_err();
    assert!(error.to_string().contains("conflicts"));
    assert_eq!(
        service
            .events_after("conflict", None, 100)
            .unwrap()
            .events
            .iter()
            .filter(|event| event.event.event_type == OperatorEventType::LegacySessionImported)
            .count(),
        1
    );
}

#[test]
fn loads_v0_transcript_and_assigns_monotonic_ids() {
    with_temp_home(|home| {
        let legacy = r#"{
            "session_id": "default",
            "transcript": [
                { "User": "hello" },
                { "Assistant": "hi" },
                { "Tool": ["sh", "ok"] },
                { "Evidence": "route ok" }
            ]
        }"#;
        write_legacy_v0(home, "default", legacy);

        let persisted = load_transcript("default").expect("load");
        assert_eq!(persisted.version, PERSISTED_TRANSCRIPT_VERSION);
        assert_eq!(persisted.session_id, "default");
        assert_eq!(persisted.parent_session_id, None);
        assert_eq!(persisted.entries.len(), 4);
        assert_eq!(persisted.next_entry_id, 4);

        let ids: Vec<u64> = persisted.entries.iter().map(|e| e.id).collect();
        assert_eq!(ids, vec![0, 1, 2, 3]);

        for e in &persisted.entries {
            assert_eq!(e.ts_unix_ms, 0, "legacy entries keep unknown timestamp");
            assert!(e.embedding_ref.is_none());
        }

        match &persisted.entries[0].block {
            TranscriptBlock::User(s) => assert_eq!(s, "hello"),
            other => panic!("expected User, got {:?}", other),
        }
        match &persisted.entries[2].block {
            TranscriptBlock::Tool(n, out) => {
                assert_eq!(n, "sh");
                assert_eq!(out, "ok");
            }
            other => panic!("expected Tool, got {:?}", other),
        }
    });
}

#[test]
fn empty_file_returns_empty_transcript() {
    with_temp_home(|_home| {
        let persisted = load_transcript("default").expect("load empty");
        assert_eq!(persisted.entries.len(), 0);
        assert_eq!(persisted.next_entry_id, 0);
        assert_eq!(persisted.version, PERSISTED_TRANSCRIPT_VERSION);
    });
}

#[test]
fn round_trips_v1_transcript() {
    with_temp_home(|_home| {
        let mut t = PersistedTranscript::empty("default");
        t.parent_session_id = Some("root-session".into());
        for (i, text) in ["one", "two", "three"].iter().enumerate() {
            t.entries.push(TranscriptEntry {
                id: [7, 19, 31][i],
                ts_unix_ms: 1_700_000_000_000 + i as i64,
                char_len: text.len(),
                block: TranscriptBlock::User((*text).into()),
                embedding_ref: (i == 0).then(|| heiwa_session::EmbeddingRef {
                    model: "m".into(),
                    dim: 3,
                    row_id: 9,
                }),
            });
        }
        t.next_entry_id = 42;
        save_entries(&t).expect("save");

        let hits = search_session_messages(Some("default"), "one", 10).expect("fts projection");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].content, "one");

        let reloaded = load_transcript("default").expect("load");
        assert_eq!(reloaded.version, PERSISTED_TRANSCRIPT_VERSION);
        assert_eq!(reloaded.parent_session_id.as_deref(), Some("root-session"));
        assert_eq!(reloaded.entries.len(), 3);
        assert_eq!(reloaded.next_entry_id, 42);
        assert_eq!(
            reloaded
                .entries
                .iter()
                .map(|entry| entry.id)
                .collect::<Vec<_>>(),
            vec![7, 19, 31]
        );
        assert_eq!(
            reloaded.entries[0].embedding_ref.as_ref().unwrap().row_id,
            9
        );
        let texts: Vec<String> = reloaded
            .entries
            .iter()
            .filter_map(|e| match &e.block {
                TranscriptBlock::User(s) => Some(s.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(texts, vec!["one", "two", "three"]);
    });
}

#[test]
fn parent_session_id_can_be_set_without_touching_entries() {
    with_temp_home(|_home| {
        append_entry("child", TranscriptBlock::User("child work".into())).unwrap();
        set_parent_session_id("child", Some("parent".into())).unwrap();

        let reloaded = load_transcript("child").unwrap();
        assert_eq!(reloaded.parent_session_id.as_deref(), Some("parent"));
        assert_eq!(reloaded.entries.len(), 1);
        assert_eq!(reloaded.next_entry_id, 1);
    });
}

#[test]
fn save_entries_updates_sqlite_fts_mirror() {
    with_temp_home(|_home| {
        save_transcript(
            "default",
            &[
                TranscriptBlock::User("searchable operator memory".into()),
                TranscriptBlock::Tool("shell".into(), "cargo test passed".into()),
            ],
        )
        .unwrap();

        assert!(get_session_index_path().exists());
        let hits = search_session_messages(Some("default"), "operator", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].role, "user");

        let tool_hits = search_session_messages(None, "cargo", 10).unwrap();
        assert_eq!(tool_hits.len(), 1);
        assert_eq!(tool_hits[0].role, "tool");
    });
}

#[test]
fn append_entry_preserves_next_id_across_reloads() {
    with_temp_home(|_home| {
        let e0 = append_entry("default", TranscriptBlock::User("a".into())).unwrap();
        let e1 = append_entry("default", TranscriptBlock::Assistant("b".into())).unwrap();
        assert_eq!((e0.id, e1.id), (0, 1));

        let reloaded = load_transcript("default").unwrap();
        assert_eq!(reloaded.next_entry_id, 2);

        let e2 = append_entry("default", TranscriptBlock::User("c".into())).unwrap();
        let e3 = append_entry("default", TranscriptBlock::Assistant("d".into())).unwrap();
        assert_eq!((e2.id, e3.id), (2, 3));

        let final_state = load_transcript("default").unwrap();
        assert_eq!(final_state.next_entry_id, 4);
        assert_eq!(final_state.entries.len(), 4);
    });
}

#[test]
fn save_transcript_shim_appends_new_blocks_only() {
    with_temp_home(|_home| {
        let blocks = vec![
            TranscriptBlock::User("first".into()),
            TranscriptBlock::Assistant("reply".into()),
        ];
        save_transcript("default", &blocks).unwrap();

        let after_first = load_transcript("default").unwrap();
        assert_eq!(after_first.entries.len(), 2);
        assert_eq!(after_first.next_entry_id, 2);
        let first_ts = after_first.entries[0].ts_unix_ms;

        let more = vec![
            TranscriptBlock::User("first".into()),
            TranscriptBlock::Assistant("reply".into()),
            TranscriptBlock::User("second".into()),
        ];
        save_transcript("default", &more).unwrap();

        let after_second = load_transcript("default").unwrap();
        assert_eq!(after_second.entries.len(), 3);
        assert_eq!(after_second.next_entry_id, 3);
        assert_eq!(
            after_second.entries[0].ts_unix_ms, first_ts,
            "existing entries keep original timestamps"
        );
        assert_eq!(after_second.entries[2].id, 2);
    });
}

#[test]
fn save_transcript_shim_rejects_truncation_after_operator_stream_cutover() {
    with_temp_home(|_home| {
        save_transcript(
            "default",
            &[
                TranscriptBlock::User("a".into()),
                TranscriptBlock::Assistant("b".into()),
                TranscriptBlock::User("c".into()),
            ],
        )
        .unwrap();
        assert_eq!(load_transcript("default").unwrap().entries.len(), 3);

        let error = save_transcript("default", &[TranscriptBlock::User("a".into())]).unwrap_err();
        assert!(error
            .to_string()
            .contains("legacy transcript truncation is unavailable after operator-stream cutover"));
        let reloaded = load_transcript("default").unwrap();
        assert_eq!(reloaded.entries.len(), 3);
        assert_eq!(
            reloaded.next_entry_id, 3,
            "truncation preserves id generator"
        );
    });
}

#[test]
fn v0_file_is_preserved_after_operator_stream_cutover() {
    with_temp_home(|home| {
        let legacy = r#"{
            "session_id": "default",
            "transcript": [{ "User": "hello" }]
        }"#;
        write_legacy_v0(home, "default", legacy);

        save_transcript("default", &[TranscriptBlock::User("hello".into())]).unwrap();

        let raw = fs::read_to_string(home.join(".heiwa/sessions/default.json")).unwrap();
        assert_eq!(raw, legacy);
    });
}
