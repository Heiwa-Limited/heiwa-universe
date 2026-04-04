use heiwa_loop::{LoopConfig, LoopController};
use tokio::sync::mpsc;

#[tokio::test]
async fn test_loop_turn_budget() {
    let config = LoopConfig {
        user_id: "test-user".to_string(),
        objective: "count to 2".to_string(),
        max_turns: 2,
        max_cost_usd: 1.0,
    };
    
    let controller = LoopController::new(config);
    let (tx, mut rx) = mpsc::channel(10);
    
    controller.run(tx).await.expect("loop failed");
    
    let mut turns = 0;
    while let Some(status) = rx.recv().await {
        if status.status == "RUNNING" {
            turns = status.current_turn;
        }
        if status.status == "COMPLETED" {
            break;
        }
    }
    
    assert_eq!(turns, 2, "Loop should have executed exactly 2 turns");
}

#[tokio::test]
async fn test_loop_cancellation() {
    let config = LoopConfig {
        user_id: "test-user".to_string(),
        objective: "run forever".to_string(),
        max_turns: 100,
        max_cost_usd: 1.0,
    };
    
    let controller = std::sync::Arc::new(LoopController::new(config));
    let (tx, mut rx) = mpsc::channel(10);
    
    let c = controller.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        c.cancel();
    });
    
    controller.run(tx).await.expect("loop failed");
    
    let mut cancelled = false;
    while let Some(status) = rx.recv().await {
        if status.status == "CANCELLED" {
            cancelled = true;
            break;
        }
    }
    
    assert!(cancelled, "Loop should have been cancelled");
}
