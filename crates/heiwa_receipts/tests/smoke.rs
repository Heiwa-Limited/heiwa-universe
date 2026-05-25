//! End-to-end smoke test for `heiwa_receipts`.
//!
//! Mirrors the cost-attribution ledger demoed on heiwa.ltd: five receipts
//! across local / oauth / api lanes, with the totals the hero readout shows.

use heiwa_receipts::{Costs, Env, RateTable, Receipt, ReceiptStore};
use tempfile::TempDir;

const RATES_TOML: &str = r#"
synced_at = "2026-05-25T11:00:00Z"

[rates.local.ollama."qwen3.5:9b"]
input_per_mtok_cad  = 0.0
output_per_mtok_cad = 0.0
[rates.local.ollama."qwen3.5:9b".counterfactual]
input_per_mtok_cad  = 0.27
output_per_mtok_cad = 0.81

[rates.local.ollama."qwen3.5:4b"]
input_per_mtok_cad  = 0.0
output_per_mtok_cad = 0.0
[rates.local.ollama."qwen3.5:4b".counterfactual]
input_per_mtok_cad  = 0.14
output_per_mtok_cad = 0.42

[rates.oauth."claude-code"."claude-sonnet-4-6"]
input_per_mtok_cad  = 0.0
output_per_mtok_cad = 0.0
[rates.oauth."claude-code"."claude-sonnet-4-6".counterfactual]
input_per_mtok_cad  = 4.05
output_per_mtok_cad = 20.25

[rates.oauth.codex."gpt-5-codex"]
input_per_mtok_cad  = 0.0
output_per_mtok_cad = 0.0
[rates.oauth.codex."gpt-5-codex".counterfactual]
input_per_mtok_cad  = 2.75
output_per_mtok_cad = 11.00

[rates.api.openrouter."claude-3.7-sonnet"]
input_per_mtok_cad  = 4.05
output_per_mtok_cad = 20.25
"#;

fn seed(store: &ReceiptStore, rates: &RateTable, session: &str) {
    let entries = [
        (
            Env::Local,
            "ollama",
            "qwen3.5:9b",
            "coding",
            80_000_i64,
            12_400_i64,
        ),
        (
            Env::Oauth,
            "claude-code",
            "claude-sonnet-4-6",
            "strategy",
            40_000,
            7_100,
        ),
        (
            Env::Oauth,
            "codex",
            "gpt-5-codex",
            "refactor",
            24_000,
            4_900,
        ),
        (
            Env::Api,
            "openrouter",
            "claude-3.7-sonnet",
            "trading",
            9_000,
            3_000,
        ),
        (Env::Local, "ollama", "qwen3.5:4b", "summarise", 3_500, 700),
    ];

    for (i, (env, provider, model, agent, tin, tout)) in entries.iter().enumerate() {
        let Costs {
            actual_cad,
            counterfactual_cad,
        } = rates
            .compute(*env, provider, model, *tin, *tout)
            .expect("rate lookup");
        let r = Receipt::new(
            1_716_640_000 + i as i64 * 60,
            *env,
            *provider,
            *model,
            *agent,
            *tin,
            *tout,
            40 + (i as i64) * 5,
            actual_cad,
            counterfactual_cad,
            session,
            None,
        );
        store.insert(&r).expect("insert");
    }
}

#[test]
fn full_marketing_demo_roundtrip() {
    let dir = TempDir::new().unwrap();
    let store = ReceiptStore::open(dir.path().join("receipts.db")).unwrap();
    let rates = RateTable::from_toml_str(RATES_TOML).unwrap();

    seed(&store, &rates, "sess-2026-05-25");

    let total = store.day_total(0).unwrap();
    assert_eq!(total.tokens, 184_600, "total tokens should be 184.6k");

    let api_actual = (9_000.0 / 1_000_000.0) * 4.05 + (3_000.0 / 1_000_000.0) * 20.25;
    assert!(
        (total.actual_cost_cad - api_actual).abs() < 1e-6,
        "actual_cost_cad should equal the API row's computed cost (others are 0): got {} expected {}",
        total.actual_cost_cad,
        api_actual
    );
    assert!(
        total.counterfactual_cost_cad > total.actual_cost_cad,
        "counterfactual must exceed actual when oauth/local lanes have counterfactual rates set"
    );

    let by_env = store.rollup_by_env(0).unwrap();
    assert_eq!(by_env.len(), 3, "expected three env buckets");
    assert_eq!(by_env[0].env, Env::Local);
    assert!((by_env[0].actual_cost_cad - 0.0).abs() < 1e-9);

    let by_agent = store.rollup_by_agent(0).unwrap();
    assert_eq!(by_agent.len(), 5);

    let by_model = store.rollup_by_model(0).unwrap();
    assert_eq!(by_model.len(), 5);

    let list = store.list(0, i64::MAX).unwrap();
    assert_eq!(list.len(), 5);
    assert!(list[0].at > list[4].at, "list should be DESC by at");

    let one = list[0].clone();
    let fetched = store.get(&one.id).unwrap().expect("receipt exists");
    assert_eq!(fetched, one);

    let header = one.header();
    assert_eq!(header.tokens_in, one.tokens_in);
    assert_eq!(header.tokens_out, one.tokens_out);
    assert_eq!(header.actual_cost_cad, one.actual_cost_cad);
    assert_eq!(header.schema_version, 1);
}

#[test]
fn schema_version_is_one() {
    let store = ReceiptStore::open_in_memory().unwrap();
    assert_eq!(store.schema_version().unwrap(), 1);
}

#[test]
fn empty_store_returns_zeros_not_errors() {
    let store = ReceiptStore::open_in_memory().unwrap();
    let total = store.day_total(0).unwrap();
    assert_eq!(total.tokens, 0);
    assert_eq!(total.actual_cost_cad, 0.0);
    assert_eq!(total.counterfactual_cost_cad, 0.0);

    assert!(store.rollup_by_env(0).unwrap().is_empty());
    assert!(store.rollup_by_agent(0).unwrap().is_empty());
    assert!(store.rollup_by_model(0).unwrap().is_empty());
}
