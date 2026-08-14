use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Deterministic strategy for recursive harness fan-out.
///
/// This is intentionally a planning primitive, not an executor. Heiwa's runtime
/// can compile this plan into leased child work later; no model-generated script
/// or unconstrained shell process is granted authority here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecursiveHarnessStrategy {
    /// Do not spawn children; parent handles the workload in its current lease.
    Inline,
    /// Small workload: one structured child task per entry.
    DirectTaskCalls,
    /// Larger workload: runtime-supervised fan-out plan over deterministic chunks.
    SupervisedPlan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecursiveHarnessEntry {
    pub entry_id: String,
    pub prompt: String,
    #[serde(default)]
    pub estimated_tokens: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecursiveHarnessConstraints {
    pub max_depth: u8,
    pub small_batch_threshold: usize,
    pub max_children: usize,
    pub max_entries_per_child: usize,
    pub output_root: String,
}

impl Default for RecursiveHarnessConstraints {
    fn default() -> Self {
        Self {
            max_depth: 3,
            small_batch_threshold: 5,
            max_children: 128,
            max_entries_per_child: 25,
            output_root: heiwa_config::HeiwaPaths::resolve()
                .runtime_root
                .join("receipts")
                .join("recursive-harness")
                .to_string_lossy()
                .into_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChildHarnessTask {
    pub task_id: String,
    pub parent_task_id: String,
    pub depth: u8,
    pub entry_ids: Vec<String>,
    pub instruction: String,
    pub output_receipt_path: String,
    pub lease_budget_share: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecursiveHarnessPlan {
    pub parent_task_id: String,
    pub depth: u8,
    pub strategy: RecursiveHarnessStrategy,
    pub child_tasks: Vec<ChildHarnessTask>,
    pub aggregate_receipt_path: String,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChildHarnessStatus {
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChildHarnessReceipt {
    pub task_id: String,
    pub parent_task_id: String,
    pub depth: u8,
    pub entry_ids: Vec<String>,
    pub status: ChildHarnessStatus,
    pub output_summary: String,
    #[serde(default)]
    pub source_spans: Vec<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AggregatedHarnessReceipt {
    pub parent_task_id: String,
    pub depth: u8,
    pub status: ChildHarnessStatus,
    pub total_children: usize,
    pub succeeded_children: usize,
    pub failed_children: usize,
    pub total_entries: usize,
    pub aggregate_receipt_path: String,
    pub child_receipts: Vec<ChildHarnessReceipt>,
}

pub fn aggregate_child_harness_receipts(
    plan: &RecursiveHarnessPlan,
    receipts: Vec<ChildHarnessReceipt>,
) -> Result<AggregatedHarnessReceipt> {
    let expected_ids: BTreeSet<&str> = plan
        .child_tasks
        .iter()
        .map(|task| task.task_id.as_str())
        .collect();
    let mut by_task_id: BTreeMap<String, ChildHarnessReceipt> = BTreeMap::new();

    for receipt in receipts {
        if !expected_ids.contains(receipt.task_id.as_str()) {
            return Err(anyhow!("unexpected child receipt: {}", receipt.task_id));
        }
        if receipt.parent_task_id != plan.parent_task_id {
            return Err(anyhow!(
                "child receipt {} has parent {}, expected {}",
                receipt.task_id,
                receipt.parent_task_id,
                plan.parent_task_id
            ));
        }
        if by_task_id
            .insert(receipt.task_id.clone(), receipt)
            .is_some()
        {
            return Err(anyhow!("duplicate child receipt"));
        }
    }

    let mut ordered = Vec::with_capacity(plan.child_tasks.len());
    for task in &plan.child_tasks {
        let receipt = by_task_id
            .remove(&task.task_id)
            .ok_or_else(|| anyhow!("missing child receipt: {}", task.task_id))?;
        ordered.push(receipt);
    }

    let succeeded_children = ordered
        .iter()
        .filter(|receipt| receipt.status == ChildHarnessStatus::Succeeded)
        .count();
    let failed_children = ordered
        .iter()
        .filter(|receipt| receipt.status == ChildHarnessStatus::Failed)
        .count();
    let total_entries = ordered.iter().map(|receipt| receipt.entry_ids.len()).sum();
    let status = if failed_children > 0 {
        ChildHarnessStatus::Failed
    } else {
        ChildHarnessStatus::Succeeded
    };

    Ok(AggregatedHarnessReceipt {
        parent_task_id: plan.parent_task_id.clone(),
        depth: plan.depth,
        status,
        total_children: ordered.len(),
        succeeded_children,
        failed_children,
        total_entries,
        aggregate_receipt_path: plan.aggregate_receipt_path.clone(),
        child_receipts: ordered,
    })
}

pub fn plan_recursive_harness(
    parent_task_id: impl AsRef<str>,
    entries: &[RecursiveHarnessEntry],
    current_depth: u8,
    constraints: RecursiveHarnessConstraints,
) -> Result<RecursiveHarnessPlan> {
    let parent_task_id = parent_task_id.as_ref().trim();
    if parent_task_id.is_empty() {
        return Err(anyhow!("parent_task_id is required"));
    }
    if entries.is_empty() {
        return Err(anyhow!(
            "recursive harness fan-out requires at least one entry"
        ));
    }

    let constraints = normalize_constraints(constraints);
    let safe_parent = sanitize_segment(parent_task_id);
    let aggregate_receipt_path = format!(
        "{}/{}/depth-{}/aggregate.json",
        constraints.output_root, safe_parent, current_depth
    );

    if current_depth >= constraints.max_depth {
        return Ok(RecursiveHarnessPlan {
            parent_task_id: parent_task_id.to_string(),
            depth: current_depth,
            strategy: RecursiveHarnessStrategy::Inline,
            child_tasks: Vec::new(),
            aggregate_receipt_path,
            reason: "max_depth_reached".to_string(),
        });
    }

    let strategy = if entries.len() <= constraints.small_batch_threshold {
        RecursiveHarnessStrategy::DirectTaskCalls
    } else {
        RecursiveHarnessStrategy::SupervisedPlan
    };

    let chunks: Vec<&[RecursiveHarnessEntry]> = match strategy {
        RecursiveHarnessStrategy::Inline => Vec::new(),
        RecursiveHarnessStrategy::DirectTaskCalls => entries.chunks(1).collect(),
        RecursiveHarnessStrategy::SupervisedPlan => {
            let chunks: Vec<&[RecursiveHarnessEntry]> =
                entries.chunks(constraints.max_entries_per_child).collect();
            if chunks.len() > constraints.max_children {
                return Err(anyhow!(
                    "fan-out budget exceeded: {} chunks needed, max_children is {}",
                    chunks.len(),
                    constraints.max_children
                ));
            }
            chunks
        }
    };

    if chunks.len() > constraints.max_children {
        return Err(anyhow!(
            "fan-out budget exceeded: {} children needed, max_children is {}",
            chunks.len(),
            constraints.max_children
        ));
    }

    let child_depth = current_depth.saturating_add(1);
    let budget_share = 1.0 / chunks.len() as f64;
    let child_tasks = chunks
        .into_iter()
        .enumerate()
        .map(|(idx, chunk)| {
            let task_id = format!("{}-d{}-c{:04}", safe_parent, child_depth, idx + 1);
            let entry_ids: Vec<String> = chunk.iter().map(|entry| entry.entry_id.clone()).collect();
            ChildHarnessTask {
                task_id: task_id.clone(),
                parent_task_id: parent_task_id.to_string(),
                depth: child_depth,
                instruction: child_instruction(parent_task_id, child_depth, &entry_ids),
                output_receipt_path: format!(
                    "{}/{}/depth-{}/{}.json",
                    constraints.output_root, safe_parent, child_depth, task_id
                ),
                entry_ids,
                lease_budget_share: budget_share,
            }
        })
        .collect();

    Ok(RecursiveHarnessPlan {
        parent_task_id: parent_task_id.to_string(),
        depth: current_depth,
        strategy,
        child_tasks,
        aggregate_receipt_path,
        reason: match strategy {
            RecursiveHarnessStrategy::DirectTaskCalls => "small_batch_direct_tasks".to_string(),
            RecursiveHarnessStrategy::SupervisedPlan => "large_batch_supervised_plan".to_string(),
            RecursiveHarnessStrategy::Inline => "inline".to_string(),
        },
    })
}

fn normalize_constraints(
    mut constraints: RecursiveHarnessConstraints,
) -> RecursiveHarnessConstraints {
    constraints.max_depth = constraints.max_depth.max(1);
    constraints.small_batch_threshold = constraints.small_batch_threshold.max(1);
    constraints.max_children = constraints.max_children.max(1);
    constraints.max_entries_per_child = constraints.max_entries_per_child.max(1);
    if constraints.output_root.trim().is_empty() {
        constraints.output_root = RecursiveHarnessConstraints::default().output_root;
    }
    constraints
}

fn child_instruction(parent_task_id: &str, depth: u8, entry_ids: &[String]) -> String {
    format!(
        "Process {} entries for parent task {} at recursive depth {}. Return a structured JSON receipt with entry_id results, source spans, confidence, and errors. Do not mutate outside the granted child lease.",
        entry_ids.len(),
        parent_task_id,
        depth
    )
}

fn sanitize_segment(input: &str) -> String {
    let mut out = String::new();
    for ch in input.chars().take(80) {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        "task".to_string()
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str) -> RecursiveHarnessEntry {
        RecursiveHarnessEntry {
            entry_id: id.to_string(),
            prompt: format!("process {id}"),
            estimated_tokens: 128,
        }
    }

    #[test]
    fn small_batches_plan_one_child_per_entry() {
        let entries = vec![entry("a"), entry("b"), entry("c")];
        let plan = plan_recursive_harness(
            "parent/one",
            &entries,
            0,
            RecursiveHarnessConstraints::default(),
        )
        .expect("plan");

        assert_eq!(plan.strategy, RecursiveHarnessStrategy::DirectTaskCalls);
        assert_eq!(plan.reason, "small_batch_direct_tasks");
        assert_eq!(plan.child_tasks.len(), 3);
        assert_eq!(plan.child_tasks[0].entry_ids, vec!["a"]);
        assert_eq!(plan.child_tasks[0].depth, 1);
        assert!(plan.child_tasks[0]
            .output_receipt_path
            .contains("parent_one/depth-1/parent_one-d1-c0001.json"));
    }

    #[test]
    fn large_batches_plan_supervised_chunks() {
        let entries: Vec<RecursiveHarnessEntry> =
            (0..12).map(|i| entry(&format!("entry-{i}"))).collect();
        let constraints = RecursiveHarnessConstraints {
            max_entries_per_child: 5,
            ..RecursiveHarnessConstraints::default()
        };
        let plan = plan_recursive_harness("parent", &entries, 0, constraints).expect("plan");

        assert_eq!(plan.strategy, RecursiveHarnessStrategy::SupervisedPlan);
        assert_eq!(plan.reason, "large_batch_supervised_plan");
        assert_eq!(plan.child_tasks.len(), 3);
        assert_eq!(plan.child_tasks[0].entry_ids.len(), 5);
        assert_eq!(plan.child_tasks[2].entry_ids.len(), 2);
        assert!(plan
            .aggregate_receipt_path
            .ends_with("parent/depth-0/aggregate.json"));
    }

    #[test]
    fn depth_cap_keeps_work_inline() {
        let entries = vec![entry("a"), entry("b")];
        let plan = plan_recursive_harness(
            "parent",
            &entries,
            3,
            RecursiveHarnessConstraints::default(),
        )
        .expect("plan");

        assert_eq!(plan.strategy, RecursiveHarnessStrategy::Inline);
        assert_eq!(plan.reason, "max_depth_reached");
        assert!(plan.child_tasks.is_empty());
    }

    #[test]
    fn fanout_budget_is_hard_limit() {
        let entries: Vec<RecursiveHarnessEntry> =
            (0..7).map(|i| entry(&format!("entry-{i}"))).collect();
        let constraints = RecursiveHarnessConstraints {
            small_batch_threshold: 1,
            max_entries_per_child: 2,
            max_children: 3,
            ..RecursiveHarnessConstraints::default()
        };
        let err = plan_recursive_harness("parent", &entries, 0, constraints)
            .expect_err("fan-out should exceed child budget");

        assert!(err.to_string().contains("fan-out budget exceeded"));
    }

    fn child_receipt(task: &ChildHarnessTask, status: ChildHarnessStatus) -> ChildHarnessReceipt {
        ChildHarnessReceipt {
            task_id: task.task_id.clone(),
            parent_task_id: task.parent_task_id.clone(),
            depth: task.depth,
            entry_ids: task.entry_ids.clone(),
            status,
            output_summary: format!("processed {} entries", task.entry_ids.len()),
            source_spans: vec!["input.json:1-2".to_string()],
            error: None,
        }
    }

    #[test]
    fn aggregate_receipts_preserves_plan_order_and_status_counts() {
        let entries = vec![entry("a"), entry("b"), entry("c")];
        let plan = plan_recursive_harness(
            "parent",
            &entries,
            0,
            RecursiveHarnessConstraints::default(),
        )
        .expect("plan");
        let receipts = vec![
            child_receipt(&plan.child_tasks[2], ChildHarnessStatus::Succeeded),
            child_receipt(&plan.child_tasks[0], ChildHarnessStatus::Succeeded),
            child_receipt(&plan.child_tasks[1], ChildHarnessStatus::Failed),
        ];

        let aggregate = aggregate_child_harness_receipts(&plan, receipts).expect("aggregate");

        assert_eq!(aggregate.parent_task_id, "parent");
        assert_eq!(aggregate.status, ChildHarnessStatus::Failed);
        assert_eq!(aggregate.total_children, 3);
        assert_eq!(aggregate.succeeded_children, 2);
        assert_eq!(aggregate.failed_children, 1);
        assert_eq!(
            aggregate
                .child_receipts
                .iter()
                .map(|r| r.task_id.as_str())
                .collect::<Vec<_>>(),
            plan.child_tasks
                .iter()
                .map(|t| t.task_id.as_str())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn aggregate_receipts_rejects_missing_or_extra_children() {
        let entries = vec![entry("a"), entry("b")];
        let plan = plan_recursive_harness(
            "parent",
            &entries,
            0,
            RecursiveHarnessConstraints::default(),
        )
        .expect("plan");

        let missing = vec![child_receipt(
            &plan.child_tasks[0],
            ChildHarnessStatus::Succeeded,
        )];
        let missing_err = aggregate_child_harness_receipts(&plan, missing)
            .expect_err("missing child receipt should fail");
        assert!(missing_err.to_string().contains("missing child receipt"));

        let mut extra = plan
            .child_tasks
            .iter()
            .map(|task| child_receipt(task, ChildHarnessStatus::Succeeded))
            .collect::<Vec<_>>();
        extra.push(ChildHarnessReceipt {
            task_id: "extra-child".to_string(),
            parent_task_id: "parent".to_string(),
            depth: 1,
            entry_ids: vec!["z".to_string()],
            status: ChildHarnessStatus::Succeeded,
            output_summary: "unexpected".to_string(),
            source_spans: Vec::new(),
            error: None,
        });

        let extra_err = aggregate_child_harness_receipts(&plan, extra)
            .expect_err("extra child receipt should fail");
        assert!(extra_err.to_string().contains("unexpected child receipt"));
    }
}
