use crate::adapter::{Message, ProviderAdapter, ProviderTools, StreamEvent, TokenUsage};
use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use futures_util::StreamExt;
use serde_json::{json, Value};
use std::time::Duration;
use tokio::sync::mpsc;

/// Structured Ollama chat transport. No terminal process, spinner, ANSI output,
/// or loss of system/user/assistant roles. The host owns every tool effect.
pub struct OllamaCliAdapter {
    default_model: String,
    endpoint: String,
    supports_tools: bool,
    context_window: u32,
}

impl OllamaCliAdapter {
    pub(crate) fn from_admitted_lane(lane: &crate::admission::RegistryAdmittedLane) -> Self {
        Self {
            default_model: lane.provider_model_id().to_string(),
            endpoint: lane
                .runtime_endpoint()
                .expect("admission validates runtime endpoint")
                .to_string(),
            supports_tools: lane.supports_tools(),
            context_window: lane.context_window(),
        }
    }

    async fn chat(
        &self,
        model: &str,
        messages: &[Message],
        tx: mpsc::Sender<StreamEvent>,
        tools: ProviderTools,
    ) -> Result<()> {
        let model = if model.is_empty() {
            &self.default_model
        } else {
            model
        };
        let endpoint = self.endpoint.trim_end_matches('/');
        let endpoint = if endpoint.contains("://") {
            endpoint.to_string()
        } else {
            format!("http://{endpoint}")
        };
        let client = reqwest::Client::builder()
            .no_proxy()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(180))
            .build()?;
        let fallback;
        let messages = if !self.supports_tools {
            fallback = crate::adapter::fallback_tool_messages(messages, &tools);
            fallback.as_slice()
        } else {
            messages
        };
        let mut history: Vec<Value> = messages
            .iter()
            .filter(|message| {
                !self.supports_tools
                    || tools.definitions.is_empty()
                    || message.content != crate::adapter::TOOL_PROTOCOL
            })
            .map(|message| serde_json::to_value(message))
            .collect::<Result<_, _>>()?;
        let definitions: Vec<Value> = tools.definitions.iter().filter(|_| self.supports_tools).map(|tool| json!({
            "type":"function", "function":{"name":tool.wire_name(),"description":tool.description,"parameters":tool.input_schema}
        })).collect();
        let mut usage = TokenUsage::default();
        for _ in 0..12 {
            let mut body = json!({"model":model,"messages":history,"stream":true,"options":{"num_ctx":self.context_window.clamp(2048,16384)}});
            if !definitions.is_empty() {
                body["tools"] = json!(definitions);
            }
            let response = client
                .post(format!("{endpoint}/api/chat"))
                .json(&body)
                .send()
                .await?
                .error_for_status()?;
            let mut stream = response.bytes_stream();
            let mut buffer = Vec::new();
            let mut answer = String::new();
            let mut calls = Vec::new();
            let mut completed = false;
            loop {
                let chunk = tokio::select! {
                    _ = tx.closed() => return Ok(()),
                    chunk = stream.next() => chunk,
                };
                let eof = chunk.is_none();
                if let Some(chunk) = chunk {
                    buffer.extend_from_slice(&chunk?);
                }
                while let Some(nl) = buffer.iter().position(|byte| *byte == b'\n') {
                    if nl > 1024 * 1024 {
                        bail!("Ollama chat frame exceeds 1 MiB");
                    }
                    let line: Vec<u8> = buffer.drain(..=nl).collect();
                    if line.iter().all(u8::is_ascii_whitespace) {
                        continue;
                    }
                    let value: Value =
                        serde_json::from_slice(&line).context("invalid Ollama chat frame")?;
                    consume_frame(
                        value,
                        &tx,
                        &mut answer,
                        &mut calls,
                        &mut usage,
                        &mut completed,
                    )
                    .await?;
                }
                if buffer.len() > 1024 * 1024 {
                    bail!("Ollama chat frame exceeds 1 MiB");
                }
                if completed {
                    break;
                }
                // A final non-newline frame is permitted by JSONL readers.
                if eof {
                    if !buffer.is_empty() {
                        let value: Value = serde_json::from_slice(&buffer)?;
                        consume_frame(
                            value,
                            &tx,
                            &mut answer,
                            &mut calls,
                            &mut usage,
                            &mut completed,
                        )
                        .await?;
                    }
                    break;
                }
            }
            if !completed {
                bail!("Ollama stream ended without a completion frame");
            }
            if calls.is_empty() {
                tx.send(StreamEvent::Done(usage)).await.ok();
                return Ok(());
            }
            history.push(json!({"role":"assistant","content":answer,"tool_calls":calls}));
            for call in calls {
                let name = call["function"]["name"]
                    .as_str()
                    .context("Ollama tool call missing name")?;
                let result = tools
                    .call(name, call["function"]["arguments"].clone())
                    .await?;
                history.push(json!({"role":"tool","tool_name":name,"content":result.output}));
            }
        }
        bail!("Ollama agent round limit reached before completion")
    }
}

async fn consume_frame(
    value: Value,
    tx: &mpsc::Sender<StreamEvent>,
    answer: &mut String,
    calls: &mut Vec<Value>,
    usage: &mut TokenUsage,
    completed: &mut bool,
) -> Result<()> {
    if let Some(error) = value["error"].as_str() {
        bail!("Ollama: {error}");
    }
    // Deliberate thinking is a separate API field; never render it as an answer.
    if let Some(text) = value["message"]["content"]
        .as_str()
        .filter(|text| !text.is_empty())
    {
        if answer.len() + text.len() > 4 * 1024 * 1024 {
            bail!("Ollama answer exceeds 4 MiB");
        }
        answer.push_str(text);
        tx.send(StreamEvent::Token(text.to_string())).await.ok();
    }
    if let Some(items) = value["message"]["tool_calls"].as_array() {
        if calls.len() + items.len() > 48 {
            bail!("Ollama tool batch exceeds limit");
        }
        calls.extend(items.iter().cloned());
    }
    if value["done"].as_bool() == Some(true) {
        if value["done_reason"].as_str() == Some("length") {
            bail!("Ollama generation hit its length limit");
        }
        usage.input_tokens = usage.input_tokens.saturating_add(
            value["prompt_eval_count"]
                .as_u64()
                .unwrap_or(0)
                .min(u32::MAX as u64) as u32,
        );
        usage.output_tokens = usage.output_tokens.saturating_add(
            value["eval_count"]
                .as_u64()
                .unwrap_or(0)
                .min(u32::MAX as u64) as u32,
        );
        *completed = true;
    }
    Ok(())
}

#[async_trait]
impl ProviderAdapter for OllamaCliAdapter {
    async fn send(
        &self,
        model: &str,
        messages: &[Message],
        tx: mpsc::Sender<StreamEvent>,
    ) -> Result<()> {
        self.send_with_tools(model, messages, tx, ProviderTools::default())
            .await
    }
    async fn send_with_tools(
        &self,
        model: &str,
        messages: &[Message],
        tx: mpsc::Sender<StreamEvent>,
        tools: ProviderTools,
    ) -> Result<()> {
        if let Err(error) = self.chat(model, messages, tx.clone(), tools).await {
            tx.send(StreamEvent::Error(format!("Ollama chat failed: {error:#}")))
                .await
                .ok();
        }
        Ok(())
    }
    async fn interrupt(&self) -> Result<()> {
        Ok(())
    }
    fn supports_native_tools(&self) -> bool {
        self.supports_tools
    }
    fn supported_models(&self) -> Vec<String> {
        vec![self.default_model.clone()]
    }
    fn execution_channel(&self) -> crate::adapter::ExecutionChannel {
        crate::adapter::ExecutionChannel::local_runtime("ollama")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::{Role, ToolDefinition, ToolResponse};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    async fn request(listener: &tokio::net::TcpListener, body: &str) -> Value {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut bytes = Vec::new();
        let mut chunk = [0u8; 4096];
        let split = loop {
            let n = stream.read(&mut chunk).await.unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&chunk[..n]);
            if let Some(i) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
                break i + 4;
            }
        };
        let header = String::from_utf8_lossy(&bytes[..split]);
        let size: usize = header
            .lines()
            .find_map(|line| {
                line.to_lowercase()
                    .strip_prefix("content-length:")
                    .map(|v| v.trim().parse().unwrap())
            })
            .unwrap();
        while bytes.len() < split + size {
            let n = stream.read(&mut chunk).await.unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&chunk[..n]);
        }
        let value = serde_json::from_slice(&bytes[split..split + size]).unwrap();
        stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/x-ndjson\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len()).as_bytes()).await.unwrap();
        // Deliberately split Unicode and JSON frames across network reads.
        for part in body.as_bytes().chunks(7) {
            stream.write_all(part).await.unwrap();
        }
        value
    }
    #[tokio::test]
    async fn native_tool_rounds_preserve_roles_results_usage_and_hide_thinking() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let first=request(&listener,&format!("{}\n{}\n",json!({"message":{"content":"","thinking":"PRIVATE_REASONING_SENTINEL","tool_calls":[{"function":{"name":"fs__read","arguments":{"path":"proof.txt"}}}]},"done":false}),json!({"done":true,"prompt_eval_count":9,"eval_count":2}))).await;
            assert_eq!(first["messages"][0]["role"], "system");
            assert_eq!(first["messages"][1]["role"], "user");
            assert_eq!(first["tools"][0]["function"]["name"], "fs__read");
            let second=request(&listener,&format!("{}\n",json!({"message":{"content":"verified é"},"done":true,"prompt_eval_count":13,"eval_count":4}))).await;
            assert_eq!(
                second["messages"].as_array().unwrap().last().unwrap(),
                &json!({"role":"tool","tool_name":"fs__read","content":"host fixture"})
            );
        });
        let (requests, mut calls) = mpsc::channel::<crate::adapter::ProviderToolRequest>(2);
        let dispatcher = tokio::spawn(async move {
            let call = calls.recv().await.unwrap();
            assert_eq!(call.name, "fs.read");
            call.response
                .send(ToolResponse {
                    output: "host fixture".into(),
                    success: true,
                })
                .unwrap();
        });
        let tools = ProviderTools {
            definitions: vec![ToolDefinition {
                name: "fs.read".into(),
                description: "read".into(),
                input_schema: json!({"type":"object","properties":{"path":{"type":"string"}}}),
            }],
            requests: Some(requests),
            ..Default::default()
        };
        let adapter = OllamaCliAdapter {
            default_model: "fixture".into(),
            endpoint,
            supports_tools: true,
            context_window: 8192,
        };
        let (tx, mut rx) = mpsc::channel(16);
        adapter
            .send_with_tools(
                "fixture",
                &[
                    Message {
                        role: Role::System,
                        content: "trusted instructions".into(),
                    },
                    Message {
                        role: Role::User,
                        content: "read fixture".into(),
                    },
                ],
                tx,
                tools,
            )
            .await
            .unwrap();
        let mut text = String::new();
        let mut usage = None;
        while let Some(event) = rx.recv().await {
            match event {
                StreamEvent::Token(t) => text.push_str(&t),
                StreamEvent::Done(u) => usage = Some(u),
                StreamEvent::Error(e) => panic!("{e}"),
                _ => {}
            }
        }
        assert_eq!(text, "verified é");
        let usage = usage.unwrap();
        assert_eq!((usage.input_tokens, usage.output_tokens), (22, 6));
        server.await.unwrap();
        dispatcher.await.unwrap();
    }
    #[tokio::test]
    async fn premature_eof_and_length_limit_are_terminal_errors() {
        for response in [
            json!({"message":{"content":"partial"}}),
            json!({"done":true,"done_reason":"length"}),
        ] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let endpoint = format!("http://{}", listener.local_addr().unwrap());
            let server = tokio::spawn(async move {
                request(&listener, &response.to_string()).await;
            });
            let adapter = OllamaCliAdapter {
                default_model: "fixture".into(),
                endpoint,
                supports_tools: true,
                context_window: 4096,
            };
            let (tx, mut rx) = mpsc::channel(16);
            adapter
                .send(
                    "fixture",
                    &[Message {
                        role: Role::User,
                        content: "hello".into(),
                    }],
                    tx,
                )
                .await
                .unwrap();
            let mut errors = 0;
            while let Some(event) = rx.recv().await {
                if matches!(event, StreamEvent::Error(_)) {
                    errors += 1;
                }
                assert!(!matches!(event, StreamEvent::Done(_)));
            }
            assert_eq!(errors, 1);
            server.await.unwrap();
        }
    }
}
