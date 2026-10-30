use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use heiwa_protocol::ExecutionScope;
use schemars::{schema::RootSchema, schema_for, JsonSchema};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{McpError, PolicyDenial, Result, Tool, ToolRegistry};

pub fn local_repo_registry(scope: ExecutionScope) -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    registry.register(FsRead::new(scope.clone()));
    registry.register(FsList::new(scope.clone()));
    registry.register(RepoGrep::new(scope));
    registry
}

#[derive(Clone)]
pub struct FsRead {
    scope: ExecutionScope,
}

impl FsRead {
    pub fn new(scope: ExecutionScope) -> Self {
        Self { scope }
    }
}

#[derive(Clone)]
pub struct FsList {
    scope: ExecutionScope,
}

impl FsList {
    pub fn new(scope: ExecutionScope) -> Self {
        Self { scope }
    }
}

#[derive(Clone)]
pub struct RepoGrep {
    scope: ExecutionScope,
}

impl RepoGrep {
    pub fn new(scope: ExecutionScope) -> Self {
        Self { scope }
    }
}

#[derive(Deserialize, JsonSchema)]
struct FsReadInput {
    path: String,
    #[serde(default = "default_max_bytes")]
    #[schemars(range(min = 1, max = 262144))]
    max_bytes: usize,
}

#[derive(Deserialize, JsonSchema)]
struct FsListInput {
    #[serde(default = "default_dot")]
    path: String,
}

#[derive(Deserialize, JsonSchema)]
struct RepoGrepInput {
    pattern: String,
    #[serde(default = "default_dot")]
    path: String,
    #[serde(default = "default_max_matches")]
    #[schemars(range(min = 1, max = 200))]
    max_matches: usize,
}

fn default_dot() -> String {
    ".".to_string()
}

fn default_max_bytes() -> usize {
    64 * 1024
}

fn default_max_matches() -> usize {
    50
}

#[async_trait]
impl Tool for FsRead {
    fn name(&self) -> &'static str {
        "fs.read"
    }

    fn description(&self) -> &'static str {
        "Read a UTF-8 file inside the active execution scope."
    }

    fn input_schema(&self) -> RootSchema {
        schema_for!(FsReadInput)
    }

    async fn call(&self, args: Value) -> Result<Value> {
        ensure_lease(&self.scope, self.name())?;
        let input: FsReadInput =
            serde_json::from_value(args).map_err(|source| McpError::InvalidArguments {
                tool: self.name().to_string(),
                source,
            })?;
        let resolved = resolve_existing_path(&self.scope, &input.path)?;
        if !resolved.absolute.is_file() {
            return Err(McpError::Tool(format!("not a file: {}", resolved.relative)));
        }
        if input.max_bytes == 0 || input.max_bytes > 256 * 1024 {
            return Err(McpError::Tool(
                "max_bytes must be between 1 and 262144".into(),
            ));
        }
        let file = fs::File::open(&resolved.absolute)
            .map_err(|error| McpError::Tool(format!("read failed: {error}")))?;
        let mut bytes = Vec::new();
        file.take(input.max_bytes as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| McpError::Tool(format!("read failed: {error}")))?;
        let truncated = bytes.len() > input.max_bytes;
        bytes.truncate(input.max_bytes);
        if let Err(error) = std::str::from_utf8(&bytes) {
            if truncated && error.error_len().is_none() {
                bytes.truncate(error.valid_up_to());
            } else {
                return Err(McpError::Tool("file is not UTF-8 text".into()));
            }
        }
        let content = String::from_utf8(bytes)
            .map_err(|_| McpError::Tool("file is not UTF-8 text".into()))?;
        Ok(json!({
            "path": resolved.relative,
            "content": content,
            "truncated": truncated,
        }))
    }
}

#[async_trait]
impl Tool for FsList {
    fn name(&self) -> &'static str {
        "fs.list"
    }

    fn description(&self) -> &'static str {
        "List a directory inside the active execution scope."
    }

    fn input_schema(&self) -> RootSchema {
        schema_for!(FsListInput)
    }

    async fn call(&self, args: Value) -> Result<Value> {
        ensure_lease(&self.scope, self.name())?;
        let input: FsListInput = if args.is_null() {
            FsListInput {
                path: default_dot(),
            }
        } else {
            serde_json::from_value(args).map_err(|source| McpError::InvalidArguments {
                tool: self.name().to_string(),
                source,
            })?
        };
        let resolved = resolve_existing_path(&self.scope, &input.path)?;
        if !resolved.absolute.is_dir() {
            return Err(McpError::Tool(format!(
                "not a directory: {}",
                resolved.relative
            )));
        }
        let mut entries = Vec::new();
        for entry in fs::read_dir(&resolved.absolute)
            .map_err(|error| McpError::Tool(format!("list failed: {error}")))?
            .take(501)
        {
            let entry = entry.map_err(|error| McpError::Tool(format!("list failed: {error}")))?;
            let path = entry.path();
            let kind = if path.is_dir() {
                "dir"
            } else if path.is_file() {
                "file"
            } else {
                "other"
            };
            entries.push(json!({
                "name": entry.file_name().to_string_lossy(),
                "path": relative_to_scope(&self.scope, &path),
                "kind": kind,
            }));
        }
        let truncated = entries.len() > 500;
        entries.truncate(500);
        entries.sort_by(|a, b| {
            a["path"]
                .as_str()
                .unwrap_or_default()
                .cmp(b["path"].as_str().unwrap_or_default())
        });
        Ok(json!({
            "path": resolved.relative,
            "entries": entries,
            "truncated":truncated,
        }))
    }
}

#[async_trait]
impl Tool for RepoGrep {
    fn name(&self) -> &'static str {
        "repo.grep"
    }

    fn description(&self) -> &'static str {
        "Search UTF-8 files inside the active execution scope for a literal pattern."
    }

    fn input_schema(&self) -> RootSchema {
        schema_for!(RepoGrepInput)
    }

    async fn call(&self, args: Value) -> Result<Value> {
        ensure_lease(&self.scope, self.name())?;
        let input: RepoGrepInput =
            serde_json::from_value(args).map_err(|source| McpError::InvalidArguments {
                tool: self.name().to_string(),
                source,
            })?;
        if input.pattern.is_empty()
            || input.pattern.len() > 1024
            || input.max_matches == 0
            || input.max_matches > 200
        {
            return Err(McpError::Tool(
                "pattern must be 1..1024 bytes and max_matches 1..200".to_string(),
            ));
        }
        let resolved = resolve_existing_path(&self.scope, &input.path)?;
        let mut matches = Vec::new();
        let mut budget = GrepBudget {
            started: std::time::Instant::now(),
            entries: 0,
            bytes: 0,
            truncated: false,
        };
        grep_path(
            &self.scope,
            &resolved.absolute,
            &input.pattern,
            input.max_matches,
            &mut matches,
            &mut budget,
            0,
        )?;
        Ok(json!({
            "path": resolved.relative,
            "pattern": input.pattern,
            "matches": matches,
            "truncated":budget.truncated || matches.len()>=input.max_matches,
            "scanned_entries":budget.entries,
            "scope":"symlinks are not followed; 2000 entries, 16 MiB total, 1 MiB/file, depth 32, five second scan budget",
        }))
    }
}

struct ResolvedPath {
    absolute: PathBuf,
    relative: String,
}

fn ensure_lease(scope: &ExecutionScope, tool: &str) -> Result<()> {
    if scope.allows_tool(tool) {
        Ok(())
    } else {
        Err(McpError::PolicyDenied(PolicyDenial::MissingLease {
            tool: tool.to_string(),
        }))
    }
}

fn resolve_existing_path(scope: &ExecutionScope, raw: &str) -> Result<ResolvedPath> {
    let raw_path = PathBuf::from(raw);
    let candidate = if raw_path.is_absolute() {
        raw_path
    } else {
        scope.working_dir.join(raw_path)
    };
    let absolute = fs::canonicalize(&candidate)
        .map_err(|error| McpError::Tool(format!("path resolve failed: {error}")))?;
    if !scope.allows_path(&absolute) {
        return Err(McpError::PolicyDenied(
            PolicyDenial::OutsideExecutionScope { path: absolute },
        ));
    }
    Ok(ResolvedPath {
        relative: relative_to_scope(scope, &absolute),
        absolute,
    })
}

fn relative_to_scope(scope: &ExecutionScope, path: &Path) -> String {
    let absolute = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    match absolute.strip_prefix(&scope.working_dir) {
        Ok(rel) if rel.as_os_str().is_empty() => ".".to_string(),
        Ok(rel) => rel.to_string_lossy().to_string(),
        Err(_) => absolute.display().to_string(),
    }
}

struct GrepBudget {
    started: std::time::Instant,
    entries: usize,
    bytes: usize,
    truncated: bool,
}

fn grep_path(
    scope: &ExecutionScope,
    path: &Path,
    pattern: &str,
    max_matches: usize,
    matches: &mut Vec<Value>,
    budget: &mut GrepBudget,
    depth: usize,
) -> Result<()> {
    if matches.len() >= max_matches {
        return Ok(());
    }
    if budget.entries >= 2000
        || budget.bytes >= 16 * 1024 * 1024
        || depth > 32
        || budget.started.elapsed() >= std::time::Duration::from_secs(5)
    {
        budget.truncated = true;
        return Ok(());
    }
    budget.entries += 1;
    let path = fs::canonicalize(path)
        .map_err(|error| McpError::Tool(format!("grep path resolve failed: {error}")))?;
    if !scope.allows_path(&path) {
        return Err(McpError::PolicyDenied(
            PolicyDenial::OutsideExecutionScope { path },
        ));
    }
    if path.is_dir() {
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            return Ok(());
        };
        if matches!(name, ".git" | "target" | "node_modules" | ".venv") {
            return Ok(());
        }
        let mut entries = fs::read_dir(&path)
            .map_err(|error| McpError::Tool(format!("grep list failed: {error}")))?
            .take(2001)
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|error| McpError::Tool(format!("grep list failed: {error}")))?;
        if entries.len() > 2000 {
            budget.truncated = true;
            entries.truncate(2000);
        }
        entries.sort_by_key(|entry| entry.path());
        for entry in entries {
            if entry
                .file_type()
                .map(|kind| kind.is_symlink())
                .unwrap_or(true)
            {
                continue;
            }
            grep_path(
                scope,
                &entry.path(),
                pattern,
                max_matches,
                matches,
                budget,
                depth + 1,
            )?;
            if matches.len() >= max_matches
                || budget.entries >= 2000
                || budget.bytes >= 16 * 1024 * 1024
            {
                break;
            }
        }
        return Ok(());
    }
    if !path.is_file() {
        return Ok(());
    }
    if fs::metadata(&path)
        .map(|metadata| metadata.len() > 1024 * 1024)
        .unwrap_or(true)
    {
        budget.truncated = true;
        return Ok(());
    }
    let Ok(file) = fs::File::open(&path) else {
        return Ok(());
    };
    let mut bytes = Vec::new();
    if file.take(1024 * 1024 + 1).read_to_end(&mut bytes).is_err() {
        return Ok(());
    }
    budget.bytes += bytes.len();
    if bytes.len() > 1024 * 1024 {
        budget.truncated = true;
        return Ok(());
    }
    let Ok(content) = String::from_utf8(bytes) else {
        return Ok(());
    };
    for (index, line) in content.lines().enumerate() {
        if line.contains(pattern) {
            matches.push(json!({
                "path": relative_to_scope(scope, &path),
                "line_number": index + 1,
                "line": line.chars().take(2048).collect::<String>(),
                "line_truncated":line.chars().count()>2048,
            }));
            if matches.len() >= max_matches {
                break;
            }
        }
    }
    Ok(())
}
