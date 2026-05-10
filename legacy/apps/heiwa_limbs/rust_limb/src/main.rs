mod module_bindings;

use module_bindings::*;
use spacetimedb_sdk::{credentials, DbContext, Table};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::sync::Arc;
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let db_name = "openclawheiwa";
    let url = "https://maincloud.spacetimedb.com";
    let agent_id = "iclaw-rust-01";
    let ollama_model = "qwen2.5-coder:7b"; 
    let ollama_url = "http://localhost:11434/api/chat";
    
    let startup_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_micros() as u64;

    println!("Starting Sovereign Agent Runtime (Brain Attached): {}...", agent_id);

    // 1. Identity Management
    let token = credentials::File::new("openclawheiwa").load().ok().flatten();

    // 2. Build Connection
    let conn = DbConnection::builder()
        .with_uri(url)
        .with_database_name(db_name)
        .with_token(token)
        .on_connect(|ctx, identity, token| {
            println!("Connected to SpacetimeDB Hub!");
            println!("Identity: {}", identity);
            if let Err(e) = credentials::File::new("openclawheiwa").save(token) {
                eprintln!("Failed to save credentials: {:?}", e);
            }
            ctx.subscription_builder()
                .on_applied(|_ctx| println!("Initial state synchronized."))
                .subscribe([
                    "SELECT * FROM agents",
                    "SELECT * FROM sessions",
                    "SELECT * FROM messages",
                    "SELECT * FROM tasks",
                ]);
        })
        .on_disconnect(|_ctx, _err| {
            eprintln!("Disconnected from Hub.");
        })
        .on_connect_error(|_ctx, err| {
            eprintln!("Connection error: {:?}", err);
        })
        .build()?;

    let conn = Arc::new(conn);
    let client = reqwest::Client::new();

    // 3. Register Callbacks
    let ai_conn = Arc::clone(&conn);
    let ai_client = client.clone();
    let ai_agent_id = agent_id.to_string();

    conn.db.messages().on_insert(move |_ctx, msg| {
        if msg.sender != "user" { return; }
        if msg.timestamp < startup_time { return; }

        let conn = Arc::clone(&ai_conn);
        let client = ai_client.clone();
        let session_id = msg.session_id.clone();
        let user_content = msg.content.clone();
        let agent_name = ai_agent_id.clone();
        
        println!("[Brain] Triggered by User: {}", user_content);

        tokio::spawn(async move {
            let payload = json!({
                "model": ollama_model,
                "messages": [
                    { "role": "system", "content": "You are iClaw-Rust, a sovereign agent runtime in the Heiwa ecosystem. Respond with technical precision and a slight stoic persona." },
                    { "role": "user", "content": user_content }
                ],
                "stream": false
            });

            match client.post(ollama_url).json(&payload).send().await {
                Ok(resp) => {
                    if let Ok(json) = resp.json::<serde_json::Value>().await {
                        if let Some(reply) = json["message"]["content"].as_str() {
                            println!("[Brain] Response generated: {}", reply);
                            if let Err(e) = conn.reducers.insert_message(session_id, agent_name, reply.to_string()) {
                                eprintln!("[Brain] Failed to post reply: {:?}", e);
                            }
                        }
                    }
                }
                Err(e) => eprintln!("[Ollama] Error: {:?}", e),
            }
        });
    });

    println!("Sovereign Loop Active. Listening for messages...");

    // 4. Concurrency: Heartbeat Loop
    let heartbeat_agent_id = agent_id.to_string();
    let heartbeat_conn = Arc::clone(&conn);
    tokio::spawn(async move {
        loop {
            let _ = heartbeat_conn.reducers.register_agent(heartbeat_agent_id.clone());
            let _ = heartbeat_conn.reducers.agent_heartbeat(heartbeat_agent_id.clone());
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
    });

    // 5. Run Worker
    conn.run_async().await?;
    Ok(())
}
