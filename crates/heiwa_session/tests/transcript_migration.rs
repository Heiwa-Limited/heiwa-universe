use heiwa_protocol::TranscriptBlock;
use heiwa_session::{
    append_entry, load_transcript, save_entries, save_transcript, PersistedTranscript,
    TranscriptEntry, PERSISTED_TRANSCRIPT_VERSION,
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
    env::set_var("HOME", &tmp);

    let result = f(&tmp);

    match original_home {
        Some(v) => env::set_var("HOME", v),
        None => env::remove_var("HOME"),
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
        for (i, text) in ["one", "two", "three"].iter().enumerate() {
            t.entries.push(TranscriptEntry {
                id: i as u64,
                ts_unix_ms: 1_700_000_000_000 + i as i64,
                char_len: text.len(),
                block: TranscriptBlock::User((*text).into()),
                embedding_ref: None,
            });
        }
        t.next_entry_id = 3;
        save_entries(&t).expect("save");

        let reloaded = load_transcript("default").expect("load");
        assert_eq!(reloaded.version, PERSISTED_TRANSCRIPT_VERSION);
        assert_eq!(reloaded.entries.len(), 3);
        assert_eq!(reloaded.next_entry_id, 3);
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
fn save_transcript_shim_truncates_on_shorter_input() {
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

        save_transcript("default", &[TranscriptBlock::User("a".into())]).unwrap();
        let reloaded = load_transcript("default").unwrap();
        assert_eq!(reloaded.entries.len(), 1);
        assert_eq!(
            reloaded.next_entry_id, 3,
            "truncation preserves id generator"
        );
    });
}

#[test]
fn v0_file_is_rewritten_as_v1_on_next_save() {
    with_temp_home(|home| {
        let legacy = r#"{
            "session_id": "default",
            "transcript": [{ "User": "hello" }]
        }"#;
        write_legacy_v0(home, "default", legacy);

        save_transcript("default", &[TranscriptBlock::User("hello".into())]).unwrap();

        let raw = fs::read_to_string(home.join(".heiwa/sessions/default.json")).unwrap();
        assert!(raw.contains("\"version\""));
        assert!(raw.contains("\"entries\""));
        assert!(!raw.contains("\"transcript\""));
    });
}
