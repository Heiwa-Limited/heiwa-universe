use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use serde_json::{json, Value};
use sha1::{Digest, Sha1};
use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::Instant;
use tiktoken_rs::{cl100k_base, CoreBPE};
use uuid::Uuid;

const SCHEMA_VERSION: &str = "heiwa_compress_receipt_v1";
const TOKENIZER_ID: &str = "cl100k_base";
const DEFAULT_MODEL: &str = "ollama/qwen3.5:9b";
const COMPRESSION_PROMPT: &str = "/no_think\nYou are a token-compression filter. Output a shorter version of the input that preserves every fact, number, name, identifier, code block, and CJK/emoji character. Convert HTML to Markdown. Drop navigation, footers, tracking pixels, and repeated boilerplate. Shorten URLs to domain plus path. Reply with the compressed content only. Do not explain, do not summarize what you did, do not wrap in code fences, do not add preface or suffix. If the input is already minimal, return it unchanged.";

#[derive(Debug, Clone)]
pub(crate) struct CompressionReceipt {
    pub compressed: String,
    pub receipt_path: String,
    pub receipt: Value,
    pub input_chars: usize,
    pub output_chars: usize,
    pub ratio: f64,
    pub input_tokens: usize,
    pub output_tokens: usize,
    pub estimated_usd_saved: f64,
}

#[derive(Debug, Clone)]
pub(crate) struct PricingInputs {
    pub target_provider: String,
    pub usd_per_million_input_tokens: f64,
    pub usd_per_million_output_tokens: f64,
    pub tokenizer_id: String,
    pub token_count_kind: String,
    pub exact_count_source: Option<String>,
}

pub fn run(args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("--help") | Some("-h") => {
            print_help();
            Ok(())
        }
        _ => compress(args),
    }
}

fn compress(args: &[String]) -> Result<()> {
    let model = flag_value(args, "--model").unwrap_or_else(|| DEFAULT_MODEL.to_string());
    let (body, source) = read_body(args)?;
    if body.trim().is_empty() {
        return Err(anyhow!("compress: empty input"));
    }

    let result = compress_text_for_source(&body, &source, &model)?;
    let input_stats = result
        .receipt
        .get("input")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let output_stats = result
        .receipt
        .get("output")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let duration_ms = result
        .receipt
        .get("duration_ms")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let chars_saved = result.input_chars as i64 - result.output_chars as i64;
    let receipt_id = result
        .receipt
        .get("receipt_id")
        .and_then(Value::as_str)
        .unwrap_or("unknown");

    if has_flag(args, "--json") {
        println!(
            "{}",
            json!({
                "command": "compress",
                "receipt_path": result.receipt_path,
                "receipt": result.receipt,
                "compressed": result.compressed,
            })
        );
    } else {
        println!("compress  receipt={receipt_id}");
        println!("  model: {model}");
        println!(
            "  input:  chars={} words={} lines={}",
            input_stats
                .get("chars")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            input_stats
                .get("words")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            input_stats
                .get("lines")
                .and_then(Value::as_u64)
                .unwrap_or(0)
        );
        println!(
            "  output: chars={} words={} lines={}",
            output_stats
                .get("chars")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            output_stats
                .get("words")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            output_stats
                .get("lines")
                .and_then(Value::as_u64)
                .unwrap_or(0)
        );
        println!(
            "  delta:  chars_saved={} ratio={:.3}  ({:.1}% retained)",
            chars_saved,
            result.ratio,
            result.ratio * 100.0
        );
        println!(
            "  tokens: {}->{} saved={} (tokenizer={})",
            result.input_tokens,
            result.output_tokens,
            result.input_tokens as i64 - result.output_tokens as i64,
            TOKENIZER_ID
        );
        println!("  duration_ms: {duration_ms}");
        println!("  receipt: {}", result.receipt_path);
        if has_flag(args, "--show") {
            println!("---compressed---");
            println!("{}", result.compressed);
        }
    }
    Ok(())
}

pub(crate) fn compress_text_for_source(
    body: &str,
    source: &str,
    model: &str,
) -> Result<CompressionReceipt> {
    compress_text_for_source_with_pricing(body, source, model, None)
}

pub(crate) fn compress_text_for_source_with_pricing(
    body: &str,
    source: &str,
    model: &str,
    pricing: Option<PricingInputs>,
) -> Result<CompressionReceipt> {
    let model_id = strip_provider_prefix(model);

    let started = Instant::now();
    let compressed = run_ollama(model_id, COMPRESSION_PROMPT, body)?;
    let duration_ms = started.elapsed().as_millis() as u64;

    let receipt_id = format!("cmp_{}", Uuid::new_v4().simple());
    let ts = Utc::now().to_rfc3339();
    let input_stats = string_stats(body);
    let output_stats = string_stats(&compressed);
    let ratio = if input_stats.chars == 0 {
        1.0
    } else {
        output_stats.chars as f64 / input_stats.chars as f64
    };
    let chars_saved = input_stats.chars as i64 - output_stats.chars as i64;
    let input_tokens = count_tokens(body);
    let output_tokens = count_tokens(&compressed);
    let tokens_saved = input_tokens as i64 - output_tokens as i64;
    let prompt_sha1 = sha1_hex(COMPRESSION_PROMPT);

    let (estimated_usd_saved, pricing_block) = match &pricing {
        Some(p) => pricing_estimate_block(tokens_saved, p),
        None => (0.0, Value::Null),
    };

    let receipt = json!({
        "schema_version": SCHEMA_VERSION,
        "receipt_id": receipt_id,
        "ts_utc": ts,
        "model": model,
        "model_id": model_id,
        "prompt_sha1": prompt_sha1,
        "input_source": source,
        "input": {
            "chars": input_stats.chars,
            "words": input_stats.words,
            "lines": input_stats.lines,
            "tokens": input_tokens,
        },
        "output": {
            "chars": output_stats.chars,
            "words": output_stats.words,
            "lines": output_stats.lines,
            "tokens": output_tokens,
        },
        "delta": {
            "chars_saved": chars_saved,
            "ratio": ratio,
            "tokens_saved": tokens_saved,
        },
        "tokenizer": TOKENIZER_ID,
        "pricing_estimate": pricing_block,
        "duration_ms": duration_ms,
    });

    let dir = receipts_dir();
    fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
    let path = dir.join(format!("{receipt_id}.json"));
    fs::write(&path, serde_json::to_string_pretty(&receipt)?)
        .with_context(|| format!("write {}", path.display()))?;

    Ok(CompressionReceipt {
        compressed,
        receipt_path: path.display().to_string(),
        receipt,
        input_chars: input_stats.chars,
        output_chars: output_stats.chars,
        ratio,
        input_tokens,
        output_tokens,
        estimated_usd_saved,
    })
}

fn tokenizer() -> &'static CoreBPE {
    static TOKENIZER: OnceLock<CoreBPE> = OnceLock::new();
    TOKENIZER.get_or_init(|| cl100k_base().expect("cl100k_base tokenizer init"))
}

pub(crate) fn count_tokens(text: &str) -> usize {
    tokenizer().encode_with_special_tokens(text).len()
}

fn pricing_estimate_block(tokens_saved: i64, pricing: &PricingInputs) -> (f64, Value) {
    let saved = (tokens_saved.max(0) as f64 / 1_000_000.0) * pricing.usd_per_million_input_tokens;
    let block = json!({
        "target_provider": pricing.target_provider,
        "usd_per_million_input_tokens": pricing.usd_per_million_input_tokens,
        "usd_per_million_output_tokens": pricing.usd_per_million_output_tokens,
        "estimated_usd_saved": saved,
        "basis": "input_tokens_saved * usd_per_million_input / 1M",
        "tokenizer_id": pricing.tokenizer_id,
        "token_count_kind": pricing.token_count_kind,
        "exact_count_source": pricing.exact_count_source,
    });
    (saved, block)
}

fn read_body(args: &[String]) -> Result<(String, String)> {
    if let Some(text) = flag_value(args, "--text") {
        return Ok((text, "text".to_string()));
    }
    if let Some(path) = flag_value(args, "--file") {
        let body = fs::read_to_string(&path).with_context(|| format!("read {path}"))?;
        return Ok((body, format!("file:{path}")));
    }
    let mut body = String::new();
    std::io::stdin()
        .read_to_string(&mut body)
        .context("read stdin")?;
    Ok((body, "stdin".to_string()))
}

fn strip_provider_prefix(model: &str) -> &str {
    model.strip_prefix("ollama/").unwrap_or(model)
}

fn run_ollama(model: &str, prompt: &str, body: &str) -> Result<String> {
    // Hits the Ollama HTTP API directly via curl. Avoids `ollama run`'s TTY
    // emissions (ANSI codes, streamed degenerate continuations) which broke
    // earlier attempts. stream=false for one-shot JSON response.
    let request_body = json!({
        "model": model,
        "prompt": format!("{prompt}\n\n{body}"),
        "stream": false,
        "think": false,
    });
    let mut child = Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            "300",
            "-H",
            "Content-Type: application/json",
            "-d",
            "@-",
            "http://localhost:11434/api/generate",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| "spawn curl for ollama HTTP API (is curl on PATH?)")?;

    {
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("curl stdin missing"))?;
        stdin
            .write_all(request_body.to_string().as_bytes())
            .context("write request body to curl stdin")?;
    }

    let output = child.wait_with_output().context("wait curl")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!(
            "curl exited with {}: {} (is ollama serving at localhost:11434?)",
            output.status,
            stderr.trim()
        ));
    }
    let body = String::from_utf8_lossy(&output.stdout).to_string();
    let parsed: Value = serde_json::from_str(&body).with_context(|| {
        format!(
            "parse ollama response (raw: {})",
            body.chars().take(200).collect::<String>()
        )
    })?;
    if let Some(err) = parsed.get("error").and_then(Value::as_str) {
        return Err(anyhow!("ollama error: {err}"));
    }
    let response = parsed
        .get("response")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("ollama response missing 'response' field"))?;
    Ok(strip_think_blocks(response).trim().to_string())
}

fn strip_think_blocks(s: &str) -> String {
    // Strip qwen3-style <think>...</think> blocks (case-insensitive, multiline).
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    loop {
        let lower = rest.to_ascii_lowercase();
        let Some(open) = lower.find("<think>") else {
            out.push_str(rest);
            break;
        };
        out.push_str(&rest[..open]);
        let after_open = open + "<think>".len();
        let tail = &lower[after_open..];
        match tail.find("</think>") {
            Some(close_rel) => {
                let close_abs = after_open + close_rel + "</think>".len();
                rest = &rest[close_abs..];
            }
            None => break,
        }
    }
    out
}

#[derive(Debug, PartialEq, Eq)]
struct StringStats {
    chars: usize,
    words: usize,
    lines: usize,
}

fn string_stats(s: &str) -> StringStats {
    StringStats {
        chars: s.chars().count(),
        words: s.split_whitespace().count(),
        lines: s.lines().count(),
    }
}

fn sha1_hex(s: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(s.as_bytes());
    let digest = hasher.finalize();
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

pub(crate) fn receipts_dir() -> PathBuf {
    let home = crate::home::heiwa_home().unwrap_or_else(|| PathBuf::from("."));
    home.join(".heiwa")
        .join("state")
        .join("evidence")
        .join("compress")
}

pub(crate) fn scan_recent_receipts(limit: usize) -> Vec<Value> {
    let dir = receipts_dir();
    let mut paths: Vec<PathBuf> = match fs::read_dir(&dir) {
        Ok(entries) => entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
            .collect(),
        Err(_) => return Vec::new(),
    };
    paths.sort_by(|a, b| {
        let ma = fs::metadata(a)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let mb = fs::metadata(b)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        mb.cmp(&ma)
    });
    paths
        .into_iter()
        .take(limit)
        .filter_map(|p| {
            let raw = fs::read_to_string(&p).ok()?;
            serde_json::from_str::<Value>(&raw).ok()
        })
        .collect()
}

pub(crate) fn compress_summary_payload() -> Value {
    let receipts = scan_recent_receipts(20);
    let mut total_in_chars: u64 = 0;
    let mut total_out_chars: u64 = 0;
    let mut total_in_tokens: u64 = 0;
    let mut total_out_tokens: u64 = 0;
    let mut total_usd_saved: f64 = 0.0;
    for r in &receipts {
        total_in_chars += r
            .get("input")
            .and_then(|v| v.get("chars"))
            .and_then(Value::as_u64)
            .unwrap_or(0);
        total_out_chars += r
            .get("output")
            .and_then(|v| v.get("chars"))
            .and_then(Value::as_u64)
            .unwrap_or(0);
        total_in_tokens += r
            .get("input")
            .and_then(|v| v.get("tokens"))
            .and_then(Value::as_u64)
            .unwrap_or(0);
        total_out_tokens += r
            .get("output")
            .and_then(|v| v.get("tokens"))
            .and_then(Value::as_u64)
            .unwrap_or(0);
        total_usd_saved += r
            .get("pricing_estimate")
            .and_then(|v| v.get("estimated_usd_saved"))
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
    }
    let cumulative_char_ratio = if total_in_chars == 0 {
        1.0
    } else {
        total_out_chars as f64 / total_in_chars as f64
    };
    let cumulative_token_ratio = if total_in_tokens == 0 {
        1.0
    } else {
        total_out_tokens as f64 / total_in_tokens as f64
    };
    json!({
        "receipts_dir": receipts_dir().display().to_string(),
        "count": receipts.len(),
        "totals": {
            "input_chars": total_in_chars,
            "output_chars": total_out_chars,
            "chars_saved": total_in_chars as i64 - total_out_chars as i64,
            "input_tokens": total_in_tokens,
            "output_tokens": total_out_tokens,
            "tokens_saved": total_in_tokens as i64 - total_out_tokens as i64,
            "estimated_usd_saved": total_usd_saved,
            "cumulative_ratio": cumulative_char_ratio,
            "cumulative_token_ratio": cumulative_token_ratio,
        },
        "tokenizer": TOKENIZER_ID,
        "recent": receipts,
    })
}

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|arg| arg == flag)
}

fn flag_value(args: &[String], flag: &str) -> Option<String> {
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == flag {
            return iter.next().cloned();
        }
    }
    None
}

fn print_help() {
    println!("heiwa compress");
    println!();
    println!("Usage:");
    println!("  heiwa compress [--text \"...\" | --file <path> | stdin] [--model ollama/<id>] [--show] [--json]");
    println!();
    println!("Routes outbound payloads through a local Ollama model to reduce tokens");
    println!("before frontier-model calls. Receipts persist in ~/.heiwa/state/evidence/compress/.");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_stats_counts_chars_words_lines() {
        let stats = string_stats("hello world\nsecond line");
        assert_eq!(
            stats,
            StringStats {
                chars: 23,
                words: 4,
                lines: 2
            }
        );
    }

    #[test]
    fn string_stats_handles_unicode_graphemes() {
        let stats = string_stats("日本語 emoji 🦀 ok");
        assert_eq!(stats.words, 4);
        assert!(stats.chars >= 13);
    }

    #[test]
    fn strip_provider_prefix_strips_ollama() {
        assert_eq!(strip_provider_prefix("ollama/qwen3.5:9b"), "qwen3.5:9b");
        assert_eq!(strip_provider_prefix("qwen3.5:9b"), "qwen3.5:9b");
    }

    #[test]
    fn sha1_hex_is_deterministic_40_chars() {
        let a = sha1_hex("compress prompt");
        let b = sha1_hex("compress prompt");
        assert_eq!(a, b);
        assert_eq!(a.len(), 40);
    }

    #[test]
    fn strip_think_blocks_removes_qwen_thinking() {
        let raw = "<think>internal reasoning here</think>actual output";
        assert_eq!(strip_think_blocks(raw), "actual output");
    }

    #[test]
    fn strip_think_blocks_handles_multiple_and_case_insensitive() {
        let raw = "<Think>a</Think>x<THINK>b</THINK>y";
        assert_eq!(strip_think_blocks(raw), "xy");
    }

    #[test]
    fn strip_think_blocks_handles_unclosed_tag_gracefully() {
        let raw = "<think>never closed and the rest dropped";
        assert_eq!(strip_think_blocks(raw), "");
    }

    #[test]
    fn strip_think_blocks_noop_when_no_tag() {
        assert_eq!(strip_think_blocks("plain output"), "plain output");
    }

    #[test]
    fn count_tokens_returns_positive_for_nonempty_text() {
        assert!(count_tokens("hello world") > 0);
        assert_eq!(count_tokens(""), 0);
    }

    #[test]
    fn count_tokens_reflects_compression_direction() {
        let long = "The quick brown fox jumps over the lazy dog. The quick brown fox jumps over the lazy dog. The quick brown fox jumps over the lazy dog.";
        let short = "fox jumps dog x3";
        assert!(count_tokens(long) > count_tokens(short));
    }

    #[test]
    fn count_tokens_handles_unicode_without_panic() {
        let n = count_tokens("日本語 🦀 émoji ok");
        assert!(n > 0);
    }

    #[test]
    fn pricing_estimate_marks_proxy_tokenizer_basis() {
        let pricing = PricingInputs {
            target_provider: "claude".to_string(),
            usd_per_million_input_tokens: 3.0,
            usd_per_million_output_tokens: 15.0,
            tokenizer_id: "cl100k_base".to_string(),
            token_count_kind: "proxy_estimate".to_string(),
            exact_count_source: Some("anthropic_messages_count_tokens_api".to_string()),
        };

        let (saved, block) = pricing_estimate_block(844, &pricing);

        assert_eq!(saved, 0.002532);
        assert_eq!(block["tokenizer_id"], "cl100k_base");
        assert_eq!(block["token_count_kind"], "proxy_estimate");
        assert_eq!(
            block["exact_count_source"],
            "anthropic_messages_count_tokens_api"
        );
    }

    #[test]
    fn pricing_estimate_never_reports_negative_savings() {
        let pricing = PricingInputs {
            target_provider: "claude".to_string(),
            usd_per_million_input_tokens: 3.0,
            usd_per_million_output_tokens: 15.0,
            tokenizer_id: "cl100k_base".to_string(),
            token_count_kind: "proxy_estimate".to_string(),
            exact_count_source: None,
        };

        let (saved, block) = pricing_estimate_block(-20, &pricing);

        assert_eq!(saved, 0.0);
        assert_eq!(block["estimated_usd_saved"], 0.0);
    }
}
