use std::fs;
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
        let mut content = fs::read_to_string(&resolved.absolute)
            .map_err(|error| McpError::Tool(format!("read failed: {error}")))?;
        let truncated = content.len() > input.max_bytes;
        if truncated {
            content.truncate(input.max_bytes);
        }
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
        entries.sort_by(|a, b| {
            a["path"]
                .as_str()
                .unwrap_or_default()
                .cmp(b["path"].as_str().unwrap_or_default())
        });
        Ok(json!({
            "path": resolved.relative,
            "entries": entries,
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
        if input.pattern.is_empty() {
            return Err(McpError::Tool("pattern is required".to_string()));
        }
        let resolved = resolve_existing_path(&self.scope, &input.path)?;
        let mut matches = Vec::new();
        grep_path(
            &self.scope,
            &resolved.absolute,
            &input.pattern,
            input.max_matches,
            &mut matches,
        )?;
        Ok(json!({
            "path": resolved.relative,
            "pattern": input.pattern,
            "matches": matches,
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
        return Err(McpError::PolicyDenied(PolicyDenial::OutsideExecutionScope {
            path: absolute,
        }));
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

fn grep_path(
    scope: &ExecutionScope,
    path: &Path,
    pattern: &str,
    max_matches: usize,
    matches: &mut Vec<Value>,
) -> Result<()> {
    if matches.len() >= max_matches {
        return Ok(());
    }
    let path = fs::canonicalize(path)
        .map_err(|error| McpError::Tool(format!("grep path resolve failed: {error}")))?;
    if !scope.allows_path(&path) {
        return Err(McpError::PolicyDenied(PolicyDenial::OutsideExecutionScope {
            path,
        }));
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
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|error| McpError::Tool(format!("grep list failed: {error}")))?;
        entries.sort_by_key(|entry| entry.path());
        for entry in entries {
            grep_path(scope, &entry.path(), pattern, max_matches, matches)?;
            if matches.len() >= max_matches {
                break;
            }
        }
        return Ok(());
    }
    if !path.is_file() {
        return Ok(());
    }
    let Ok(content) = fs::read_to_string(&path) else {
        return Ok(());
    };
    for (index, line) in content.lines().enumerate() {
        if line.contains(pattern) {
            matches.push(json!({
                "path": relative_to_scope(scope, &path),
                "line_number": index + 1,
                "line": line,
            }));
            if matches.len() >= max_matches {
                break;
            }
        }
    }
    Ok(())
}
