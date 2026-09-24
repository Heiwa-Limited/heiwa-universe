//! The routing-evaluation harness through the real Work-scoped runner: a
//! scripted provider proves the wiring (recorded as `synthetic`), and an
//! ignored live smoke runs one bounded episode on a local Ollama model.
//! Run every test here with disposable HEIWA_HOME, HEIWA_STATE_DIR, and
//! HEIWA_EVIDENCE_DIR.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use heiwa_core::drex::{CostTruth, ExecutionLocality, ModelCallCandidate};
use heiwa_evidence::OperatorJournal;
use heiwa_protocol::ModelTier;
use heiwa_provider::adapter::{Message, ProviderAdapter, StreamEvent, TokenUsage};
use heiwa_session::operator::OperatorSessionService;
use heiwa_shell::model_calls::ModelCallExecutor;
use heiwa_shell::operator::OperatorTurnRunner;
use heiwa_shell::routing_eval::{
    append_label, check, digesting, fixture_digest, machine_reviewer, persist_episode, run_episode,
    Deliveries, Episode, Setup, EVAL_STREAM,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::sync::mpsc;

const CORRECT: &str = r#"{"functions": [
  {"name": "retry_delay_ms", "file": "src/retry.rs"},
  {"name": "retry_allowed", "file": "src/retry.rs"},
  {"name": "retry_post", "file": "src/ledger.rs"},
  {"name": "retry_delay_doubles", "file": "tests/retry_test.rs"}
]}"#;

#[test]
fn the_rubric_labels_exact_sets_and_never_repairs() {
    let label = |output: Option<&str>| check(output)["label"].as_str().unwrap().to_string();
    assert_eq!(label(Some(CORRECT)), "pass");
    assert_eq!(label(Some(&format!("```json\n{CORRECT}\n```"))), "pass");
    assert_eq!(
        label(Some(
            &CORRECT.replace("\"src/retry.rs\"", "\"./src/retry.rs\"")
        )),
        "pass"
    );
    assert_eq!(label(None), "unjudged");
    assert_eq!(label(Some(&format!("Here you go:\n{CORRECT}"))), "fail");
    assert_eq!(label(Some(r#"{"functions": "retry_post"}"#)), "fail");

    let mut extra: Value = serde_json::from_str(CORRECT).unwrap();
    extra["comment"] = json!("extra field");
    assert_eq!(check(Some(&extra.to_string()))["reason"], "wrong_shape");
    let mut extra: Value = serde_json::from_str(CORRECT).unwrap();
    extra["functions"][0]["comment"] = json!("extra field");
    assert_eq!(check(Some(&extra.to_string()))["reason"], "wrong_shape");
    let mut duplicate: Value = serde_json::from_str(CORRECT).unwrap();
    let first = duplicate["functions"][0].clone();
    duplicate["functions"].as_array_mut().unwrap().push(first);
    assert_eq!(
        check(Some(&duplicate.to_string()))["reason"],
        "duplicate_entries"
    );

    let missing = check(Some(
        r#"{"functions": [{"name": "retry_post", "file": "src/ledger.rs"}]}"#,
    ));
    assert_eq!(missing["reason"], "wrong_set");
    let exact = &missing["checks"][3];
    assert_eq!(exact["missing"].as_array().unwrap().len(), 3);

    // A mention is not a definition, and odd text is never echoed back.
    let extra = check(Some(&CORRECT.replace(
        "]}",
        r#", {"name": "retry_budget", "file": "README.md"}, {"name": "x y", "file": "<script>"}]}"#,
    )));
    let unexpected = extra["checks"][3]["unexpected"].as_array().unwrap();
    assert!(unexpected.contains(&json!("retry_budget @ README.md")));
    assert!(unexpected
        .iter()
        .any(|item| item.as_str().unwrap().starts_with("<unrenderable ")));
    assert!(!extra.to_string().contains("<script>"));
}

/// Asks `repo.grep` for definitions, then answers with `answer`.
struct ScriptedProvider {
    calls: AtomicUsize,
    answer: String,
}

#[async_trait]
impl ProviderAdapter for ScriptedProvider {
    async fn send(
        &self,
        _model: &str,
        _messages: &[Message],
        stream_tx: mpsc::Sender<StreamEvent>,
    ) -> anyhow::Result<()> {
        let text = if self.calls.fetch_add(1, Ordering::SeqCst) == 0 {
            r#"{"tool_calls":[{"id":"grep-1","name":"repo.grep","arguments":{"pattern":"fn retry_","path":".","max_matches":50}}]}"#.to_string()
        } else {
            self.answer.clone()
        };
        stream_tx.send(StreamEvent::Token(text)).await?;
        stream_tx
            .send(StreamEvent::Done(TokenUsage::default()))
            .await?;
        Ok(())
    }

    async fn interrupt(&self) -> anyhow::Result<()> {
        Ok(())
    }

    fn supported_models(&self) -> Vec<String> {
        vec![]
    }
}

fn local_candidate(provider: &str, model: &str) -> ModelCallCandidate {
    ModelCallCandidate {
        tier: ModelTier {
            id: 1,
            model_id: model.into(),
            provider_model_id: model.into(),
            provider: provider.into(),
            rate_group: "local_ollama".into(),
            capability_class: 1,
            max_context_tokens: 32_000,
            enabled: true,
            last_success_rate: 1.0,
            ..ModelTier::default()
        },
        locality: ExecutionLocality::OnDevice,
        connected: true,
        adapter_capable: true,
        quota_available: true,
        marginal_cost_usd: Some(0.0),
        cost_truth: CostTruth::LocalZeroCost,
    }
}

/// The checkout the harness ran from, and whether it had tracked changes.
fn revision() -> Value {
    let dir = env!("CARGO_MANIFEST_DIR");
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
    };
    json!({
        "commit": git(&["rev-parse", "HEAD"]),
        "tracked_changes": git(&["status", "--porcelain", "--untracked-files=no"])
            .map(|status| !status.is_empty()),
    })
}

fn stream(root: &Path) -> Vec<Value> {
    heiwa_evidence::read_stream(root, EVAL_STREAM)
        .expect("stream")
        .events
        .into_iter()
        .map(|event| event.record)
        .collect()
}

async fn episode_with(
    provider: Arc<dyn ProviderAdapter>,
    root: &Path,
    evidence_class: &'static str,
    candidate: ModelCallCandidate,
    identity: Value,
    timeout: Duration,
) -> Episode {
    let sessions = Arc::new(OperatorSessionService::new(
        OperatorJournal::new(root.to_path_buf()).unwrap(),
    ));
    let deliveries = Deliveries::default();
    let wrapped = digesting(provider, deliveries.clone());
    let executor = Arc::new(ModelCallExecutor::new(
        Arc::new(move |_: &str, _: &str| Some(wrapped.clone())),
        sessions.clone(),
    ));
    let runner = OperatorTurnRunner::new(sessions.clone(), executor);
    let workspace = tempfile::tempdir().unwrap();
    let id = uuid::Uuid::new_v4();
    run_episode(
        &runner,
        &sessions,
        workspace.path(),
        &deliveries,
        Setup {
            evidence_class,
            work_id: format!("work-eval-{id}"),
            thread_id: format!("thread-eval-{id}"),
            candidates: vec![candidate],
            timeout,
            model_identity: identity,
            harness_revision: revision(),
        },
    )
    .await
    .expect("episode")
}

async fn synthetic(root: &Path, answer: &str) -> Episode {
    episode_with(
        Arc::new(ScriptedProvider {
            calls: AtomicUsize::new(0),
            answer: answer.to_string(),
        }),
        root,
        "synthetic",
        local_candidate("fixture", "fixture-model"),
        json!({ "status": "synthetic", "requested": "fixture-model" }),
        Duration::from_secs(10),
    )
    .await
}

#[tokio::test]
async fn a_synthetic_episode_binds_work_turn_receipts_and_digests() {
    let root = tempfile::tempdir().unwrap();
    let episode = synthetic(root.path(), CORRECT).await;
    let record = persist_episode(root.path(), episode.record).expect("persist");
    let label = append_label(
        root.path(),
        &record,
        episode.output.as_deref(),
        machine_reviewer(&record["harness"]["revision"]),
    )
    .expect("label");

    assert_eq!(record["record"], "episode", "{record:#}");
    assert_eq!(record["evidence_class"], "synthetic");
    assert_eq!(record["digests"]["fixture"], json!(fixture_digest()));
    assert_eq!(
        record["digests"]["deliveries"].as_array().unwrap().len(),
        2,
        "the first stage and the tool follow-up"
    );
    assert_eq!(
        record["digests"]["input"],
        record["digests"]["deliveries"][0]
    );
    assert_eq!(
        record["digests"]["output"],
        json!(format!("sha256:{:x}", Sha256::digest(CORRECT.as_bytes())))
    );
    assert!(record["work"]["work_id"]
        .as_str()
        .unwrap()
        .starts_with("work-eval-"));
    assert!(record["work"]["turn_id"].is_string());
    assert!(!record["receipts"].as_array().unwrap().is_empty());
    assert_eq!(record["episode"]["terminal"], "completed");
    assert_eq!(record["episode"]["model_calls"], 2);
    assert_eq!(record["episode"]["retries"], 0);
    assert_eq!(
        record["episode"]["tool_calls"],
        json!([{ "name": "repo.grep", "status": "success", "artifact": false }])
    );
    assert_eq!(record["cost"]["execution"]["truth"], "known_zero");
    assert_eq!(record["constraints"]["writable_dirs"], 0);
    assert_eq!(
        record["constraints"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["fs.list", "fs.read", "repo.grep"]
    );

    assert_eq!(label["record"], "label");
    assert_eq!(label["label"], "pass", "{label:#}");
    assert_eq!(label["workflow_accepted"], true, "{label:#}");
    assert_eq!(label["output_digest"], record["digests"]["output"]);
    assert_eq!(label["episode_id"], record["episode_id"]);
    assert_eq!(label["reviewer"]["kind"], "machine_rubric");
    assert_eq!(stream(root.path()).len(), 2);

    // Same task version, same fixture, same delivered input: replayable.
    let again = tempfile::tempdir().unwrap();
    let replay = synthetic(again.path(), CORRECT).await;
    assert_eq!(
        replay.record["digests"]["fixture"],
        record["digests"]["fixture"]
    );
    assert_eq!(
        replay.record["digests"]["input"],
        record["digests"]["input"]
    );
}

#[tokio::test]
async fn a_wrong_answer_is_labelled_fail_at_its_own_digest() {
    let root = tempfile::tempdir().unwrap();
    let wrong = r#"{"functions": [{"name": "retry_post", "file": "src/ledger.rs"}]}"#;
    let episode = synthetic(root.path(), wrong).await;
    let record = persist_episode(root.path(), episode.record).expect("persist");
    assert!(
        append_label(
            root.path(),
            &record,
            Some(CORRECT),
            json!({ "kind": "test" })
        )
        .is_err(),
        "a label cannot move to text the episode did not produce"
    );
    let label = append_label(
        root.path(),
        &record,
        episode.output.as_deref(),
        machine_reviewer(&record["harness"]["revision"]),
    )
    .expect("label");
    assert_eq!(label["label"], "fail");
    assert_eq!(label["reason"], "wrong_set");
    assert_eq!(
        label["output_digest"],
        json!(format!("sha256:{:x}", Sha256::digest(wrong.as_bytes())))
    );
}

#[tokio::test]
async fn a_correct_answer_does_not_make_an_incomplete_workflow_accepted() {
    let root = tempfile::tempdir().unwrap();
    let episode = synthetic(root.path(), CORRECT).await;
    for (field, value) in [
        ("terminal", json!("interrupted")),
        ("cancel_requested", json!(true)),
        ("timed_out", json!(true)),
        ("tool_calls", json!([])),
        (
            "tool_calls",
            json!([{ "name": "fs.read", "status": "denied" }]),
        ),
    ] {
        let mut record = episode.record.clone();
        record["episode"][field] = value;
        let label = append_label(
            root.path(),
            &record,
            episode.output.as_deref(),
            machine_reviewer(&record["harness"]["revision"]),
        )
        .expect("label");
        assert_eq!(label["label"], "pass", "answer content still passes");
        assert_eq!(label["workflow_accepted"], false, "{field}: {label:#}");
    }
}

// ---- live: one bounded episode on a local model ------------------------------

const OLLAMA_BASE: &str = "http://127.0.0.1:11434";

/// The content digest the local tag resolves to right now, if any.
async fn ollama_digest(model: &str) -> Option<String> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(5))
        .build()
        .ok()?;
    let tags: Value = client
        .get(format!("{OLLAMA_BASE}/api/tags"))
        .send()
        .await
        .ok()?
        .json()
        .await
        .ok()?;
    tags["models"]
        .as_array()?
        .iter()
        .find(|entry| entry["name"] == model || entry["model"] == model)?["digest"]
        .as_str()
        .map(|digest| format!("sha256:{digest}"))
}

/// Refuses every provider but the local Ollama CLI adapter.
fn local_only_resolver(
    deliveries: Deliveries,
) -> impl Fn(&str, &str) -> Option<Arc<dyn ProviderAdapter>> + Send + Sync + 'static {
    move |provider: &str, model: &str| {
        if provider != "ollama" {
            return None;
        }
        heiwa_provider::routing::resolve_adapter(provider, model)
            .ok()
            .map(|adapter| digesting(adapter, deliveries.clone()))
    }
}

#[tokio::test]
#[ignore = "runs a local Ollama model; needs HEIWA_EVAL_LIVE_MODEL and a disposable HEIWA_EVIDENCE_DIR"]
async fn live_local_model_smoke() {
    assert_eq!(
        std::env::var("OLLAMA_HOST").as_deref(),
        Ok(OLLAMA_BASE),
        "set OLLAMA_HOST explicitly so the CLI and digest probes use the same loopback server"
    );
    let model = std::env::var("HEIWA_EVAL_LIVE_MODEL").expect("HEIWA_EVAL_LIVE_MODEL");
    let root = PathBuf::from(std::env::var("HEIWA_EVIDENCE_DIR").expect("HEIWA_EVIDENCE_DIR"));
    let real = PathBuf::from(std::env::var("HOME").expect("HOME")).join(".heiwa");
    assert!(
        !root.starts_with(&real),
        "refusing to write live evidence under the real Heiwa home"
    );
    std::fs::create_dir_all(&root).unwrap();

    let before = ollama_digest(&model).await;
    let sessions = Arc::new(OperatorSessionService::new(
        OperatorJournal::new(root.clone()).unwrap(),
    ));
    let deliveries = Deliveries::default();
    let executor = Arc::new(ModelCallExecutor::new(
        Arc::new(local_only_resolver(deliveries.clone())),
        sessions.clone(),
    ));
    let runner = OperatorTurnRunner::new(sessions.clone(), executor);
    let workspace = tempfile::tempdir().unwrap();
    let id = uuid::Uuid::new_v4();
    let episode = run_episode(
        &runner,
        &sessions,
        workspace.path(),
        &deliveries,
        Setup {
            evidence_class: "live_local_smoke",
            work_id: format!("work-eval-{id}"),
            thread_id: format!("thread-eval-{id}"),
            candidates: vec![local_candidate("ollama", &model)],
            timeout: Duration::from_secs(240),
            model_identity: Value::Null,
            harness_revision: revision(),
        },
    )
    .await
    .expect("episode");
    let after = ollama_digest(&model).await;

    let mut record = episode.record;
    record["model"]["identity"] = json!({
        "status": match (&before, &after) {
            (Some(before), Some(after)) if before == after => "observed_digest_stable",
            _ => "observed_digest_changed_or_missing",
        },
        "requested": model,
        "endpoint": OLLAMA_BASE,
        "digest_before": before,
        "digest_after": after,
        "note": "a local tag can move; the digest was observed before and after, not pinned",
    });
    let record = persist_episode(&root, record).expect("persist");
    let label = append_label(
        &root,
        &record,
        episode.output.as_deref(),
        machine_reviewer(&record["harness"]["revision"]),
    )
    .expect("label");
    println!("EPISODE {}", serde_json::to_string_pretty(&record).unwrap());
    println!("LABEL {}", serde_json::to_string_pretty(&label).unwrap());
    println!("OUTPUT {}", serde_json::to_string(&episode.output).unwrap());
    println!("EVIDENCE_ROOT {}", root.display());
    assert_eq!(record["digests"]["fixture"], json!(fixture_digest()));
    assert!(record["work"]["turn_id"].is_string());
    assert!(label["label"].is_string());
}
