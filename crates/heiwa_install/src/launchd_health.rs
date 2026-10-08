//! Report-only health of Heiwa-owned launchd agents.
//!
//! Doctor reads `~/Library/LaunchAgents` plists whose label starts with
//! `com.heiwa.` or `ltd.heiwa.`, the matching `launchctl` service state, the
//! persisted disable overrides, and the listener on a supervised local port.
//! Nothing here mutates launchd, files, or provider state. Reports carry the
//! program path and missing required inputs only: no other argv, environment,
//! or logs. A failed or incomplete probe degrades the report; it never reads
//! as healthy.

use serde::Serialize;
use serde_json::Value;
use std::path::{Path, PathBuf};

const LABEL_PREFIXES: [&str; 2] = ["com.heiwa.", "ltd.heiwa."];
const MAX_AGENTS: usize = 32;
const OLLAMA_DEFAULT_PORT: u16 = 11434;
/// Plist files scanned for a Heiwa `Label`; more leaves the inventory incomplete.
const MAX_PLISTS: usize = 256;
/// Directory entries examined while looking for plists.
const MAX_DIRECTORY_ENTRIES: usize = 2048;

/// Interpreter options that decide whether the first operand is a script.
/// Inline/module options mean no script file; options outside the known
/// self-contained set may consume the next operand, so inference stops there.
struct LauncherOptions {
    names: &'static [&'static str],
    /// Short options that make the job inline or module code (attached or not).
    inline_short: &'static str,
    /// Short options that take no value and may be clustered.
    flag_short: &'static str,
    inline_long: &'static [&'static str],
    flag_long: &'static [&'static str],
}

const LAUNCHERS: [LauncherOptions; 6] = [
    LauncherOptions {
        names: &["node", "nodejs", "bun"],
        inline_short: "ep",
        flag_short: "",
        inline_long: &["--eval", "--print", "--input-type"],
        flag_long: &[
            "--enable-source-maps",
            "--no-warnings",
            "--trace-warnings",
            "--expose-gc",
            "--no-deprecation",
            "--trace-uncaught",
        ],
    },
    LauncherOptions {
        names: &["python", "python3"],
        inline_short: "cm",
        flag_short: "bBdEiIOqsSuvx",
        inline_long: &[],
        flag_long: &[],
    },
    LauncherOptions {
        names: &["bash", "sh", "zsh"],
        inline_short: "c",
        flag_short: "aefhlnuvx",
        inline_long: &[],
        flag_long: &["--login", "--noprofile", "--norc", "--posix"],
    },
    LauncherOptions {
        names: &["ruby"],
        inline_short: "e",
        flag_short: "dvwW",
        inline_long: &[],
        flag_long: &[],
    },
    LauncherOptions {
        names: &["perl"],
        inline_short: "eE",
        flag_short: "tTwW",
        inline_long: &[],
        flag_long: &[],
    },
    LauncherOptions {
        names: &["osascript"],
        inline_short: "e",
        flag_short: "",
        inline_long: &[],
        flag_long: &[],
    },
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PortListener {
    pub pid: u32,
    pub command: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PortConflict {
    pub port: u16,
    pub listener_pid: u32,
    pub listener_command: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ServiceState {
    pub state: Option<String>,
    pub pid: Option<u32>,
    pub runs: Option<u64>,
    /// `None` when launchd reports the job has never exited.
    pub last_exit_code: Option<i64>,
}

/// What `KeepAlive` establishes. Dictionaries are conditions, not a promise
/// to restart: only a lone `SuccessfulExit` is decidable from an exit code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RestartPolicy {
    None,
    Always,
    OnFailure,
    OnSuccess,
    Conditional,
}

impl RestartPolicy {
    fn from_plist(value: &Value) -> Self {
        match value {
            Value::Null | Value::Bool(false) => Self::None,
            Value::Bool(true) => Self::Always,
            Value::Object(conditions) if conditions.is_empty() => Self::None,
            Value::Object(conditions) if conditions.len() == 1 => {
                match conditions.get("SuccessfulExit") {
                    Some(Value::Bool(false)) => Self::OnFailure,
                    Some(Value::Bool(true)) => Self::OnSuccess,
                    _ => Self::Conditional,
                }
            }
            _ => Self::Conditional,
        }
    }

    /// Whether launchd restarts the job after this exit code; `None` when
    /// conditions outside the exit code decide.
    fn restarts_after(self, exit_code: i64) -> Option<bool> {
        match self {
            Self::None => Some(false),
            Self::Always => Some(true),
            Self::OnFailure => Some(exit_code != 0),
            Self::OnSuccess => Some(exit_code == 0),
            Self::Conditional => None,
        }
    }

    /// `SuccessfulExit` implies a first run at load (launchd.plist(5)).
    fn starts_at_load(self) -> Option<bool> {
        match self {
            Self::None => Some(false),
            Self::Always | Self::OnFailure | Self::OnSuccess => Some(true),
            Self::Conditional => None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct LaunchdAgentHealth {
    pub label: String,
    pub plist_path: PathBuf,
    pub program: Option<String>,
    /// `None` when the program is not an absolute path and cannot be checked.
    pub program_exists: Option<bool>,
    /// Missing WorkingDirectory or launcher script; other operands may be
    /// outputs and are never inferred as required.
    pub missing_paths: Vec<String>,
    /// `None` when the service probe failed.
    pub loaded: Option<bool>,
    /// Persisted override, otherwise the plist's `Disabled` default; unknown
    /// when overrides could not be read.
    pub disabled: Option<bool>,
    pub run_at_load: bool,
    pub restart_policy: RestartPolicy,
    /// Loads at next login: present and not persistently disabled.
    pub reloads_at_login: Option<bool>,
    pub service: Option<ServiceState>,
    pub port_conflict: Option<PortConflict>,
    pub classification: String,
    pub findings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LaunchdHealthReport {
    /// `ok`, `attention`, `degraded` (some evidence missing), `unavailable`,
    /// or `unsupported`. `attention` wins over `degraded`; check
    /// `evidence_complete` as well.
    pub status: String,
    pub evidence_complete: bool,
    pub platform: String,
    pub scope: String,
    pub agents: Vec<LaunchdAgentHealth>,
    /// Readable LaunchAgents plists whose `Label` is not Heiwa-owned. Only
    /// the count is reported.
    pub other_plists: usize,
    pub probe_errors: Vec<String>,
}

impl LaunchdHealthReport {
    fn empty(status: &str) -> Self {
        Self {
            status: status.to_string(),
            evidence_complete: status == "ok",
            platform: std::env::consts::OS.to_string(),
            scope: "user LaunchAgents with com.heiwa.* or ltd.heiwa.* labels".to_string(),
            agents: Vec::new(),
            other_plists: 0,
            probe_errors: Vec::new(),
        }
    }

    pub fn needs_attention(&self) -> bool {
        self.status == "attention"
    }
}

/// Each candidate plist path with its JSON form, or why it could not be read.
pub type AgentPlists = Vec<(PathBuf, Result<Value, String>)>;

/// Every `*.plist` in scope, parsed so ownership is decided by `Label`, plus
/// anything that kept the inventory from being complete.
#[derive(Debug, Clone, Default)]
pub struct Inventory {
    pub plists: AgentPlists,
    pub incomplete: Vec<String>,
}

/// Read-only sources for the assessment. The system probe shells out to
/// `plutil`, `launchctl`, and `lsof` under one time budget; tests substitute
/// fixtures. An `Err` means the evidence is missing or incomplete.
pub trait LaunchdProbe {
    fn agent_plists(&self) -> Result<Inventory, String>;
    /// `launchctl print` text for a loaded label; `Ok(None)` only when launchd
    /// reports the service does not exist.
    fn service(&self, label: &str) -> Result<Option<String>, String>;
    /// Complete `launchctl print-disabled` text for the user domain.
    fn disabled_overrides(&self) -> Result<String, String>;
    fn listener(&self, port: u16) -> Result<Option<PortListener>, String>;
    fn path_exists(&self, path: &Path) -> bool;
}

pub fn check_launchd_health() -> LaunchdHealthReport {
    #[cfg(target_os = "macos")]
    {
        match system::SystemProbe::new(system::TOTAL_BUDGET) {
            Ok(probe) => assess(&probe),
            Err(error) => {
                let mut report = LaunchdHealthReport::empty("unavailable");
                report.probe_errors.push(error);
                report
            }
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        LaunchdHealthReport::empty("unsupported")
    }
}

pub fn assess(probe: &dyn LaunchdProbe) -> LaunchdHealthReport {
    let mut report = LaunchdHealthReport::empty("ok");
    let inventory = match probe.agent_plists() {
        Ok(inventory) => inventory,
        Err(error) => {
            report.status = "unavailable".to_string();
            report.evidence_complete = false;
            report.probe_errors.push(error);
            return report;
        }
    };
    let overrides = match probe.disabled_overrides() {
        Ok(text) => Some(parse_disabled_overrides(&text)),
        Err(error) => {
            report
                .probe_errors
                .push(format!("disable overrides: {error}"));
            None
        }
    };
    report.probe_errors.extend(inventory.incomplete);
    let mut unreadable: Vec<(String, String)> = Vec::new();
    let mut heiwa_plists = Vec::new();
    for (plist_path, parsed) in inventory.plists {
        let name = plist_path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        match parsed {
            Ok(plist) if plist["Label"].as_str().is_some_and(is_heiwa_label) => {
                heiwa_plists.push((plist_path, plist))
            }
            Ok(_) => report.other_plists += 1,
            Err(error) => unreadable.push((error, name)),
        }
    }
    // Ownership of an unreadable plist is unknown, so it may hide an agent.
    unreadable.sort();
    for (error, names) in unreadable
        .chunk_by(|left, right| left.0 == right.0)
        .map(|group| (&group[0].0, group.iter().map(|(_, name)| name.as_str())))
    {
        let names: Vec<&str> = names.collect();
        let shown = names.iter().take(5).copied().collect::<Vec<_>>().join(", ");
        let more = names.len().saturating_sub(5);
        let more = if more > 0 {
            format!(" and {more} more")
        } else {
            String::new()
        };
        report
            .probe_errors
            .push(format!("ownership unknown, {error}: {shown}{more}"));
    }
    if heiwa_plists.len() > MAX_AGENTS {
        report.probe_errors.push(format!(
            "{} Heiwa agents beyond the {MAX_AGENTS}-agent cap were not assessed",
            heiwa_plists.len() - MAX_AGENTS
        ));
        heiwa_plists.truncate(MAX_AGENTS);
    }
    let mut listener_cache: Vec<(u16, Result<Option<PortListener>, String>)> = Vec::new();
    let mut attention = false;

    for (plist_path, plist) in heiwa_plists {
        let label = plist["Label"].as_str().unwrap_or_default().to_string();
        let label = label.as_str();

        let arguments: Vec<&str> = plist["ProgramArguments"]
            .as_array()
            .map(|items| items.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        let program = plist["Program"]
            .as_str()
            .or_else(|| arguments.first().copied())
            .map(str::to_string);
        let program_exists = program
            .as_deref()
            .filter(|program| program.starts_with('/'))
            .map(|program| probe.path_exists(Path::new(program)));
        let missing_paths: Vec<String> = required_inputs(
            program.as_deref(),
            &arguments,
            plist["WorkingDirectory"].as_str(),
        )
        .into_iter()
        .filter(|path| !probe.path_exists(Path::new(path)))
        .map(str::to_string)
        .collect();
        let run_at_load = plist["RunAtLoad"].as_bool().unwrap_or(false);
        let restart_policy = RestartPolicy::from_plist(&plist["KeepAlive"]);
        let starts_at_login = if run_at_load {
            Some(true)
        } else {
            restart_policy.starts_at_load()
        };
        let disabled = overrides.as_ref().map(|overrides| {
            overrides
                .iter()
                .find_map(|(name, disabled)| (name == label).then_some(*disabled))
                .unwrap_or_else(|| plist["Disabled"].as_bool().unwrap_or(false))
        });
        // Persisted enable/disable overrides take precedence over the plist
        // default. Unknown overrides leave login loading unknown.
        let reloads_at_login = disabled.map(|disabled| !disabled);

        let mut findings = Vec::new();
        let (loaded, service) = match probe.service(label) {
            Ok(Some(text)) => (Some(true), Some(parse_service(&text))),
            Ok(None) => (Some(false), None),
            Err(error) => {
                report.probe_errors.push(format!("{label}: {error}"));
                findings.push("service state unavailable".to_string());
                (None, None)
            }
        };

        let port_conflict = if program
            .as_deref()
            .and_then(|program| Path::new(program).file_name())
            .is_some_and(|name| name == "ollama")
            && (loaded == Some(true) || reloads_at_login == Some(true))
        {
            let port = ollama_port(&plist);
            let cached = match listener_cache.iter().find(|(cached, _)| *cached == port) {
                Some((_, result)) => result.clone(),
                None => {
                    let result = probe.listener(port);
                    if let Err(error) = &result {
                        report.probe_errors.push(format!("port {port}: {error}"));
                    }
                    listener_cache.push((port, result.clone()));
                    result
                }
            };
            let own_pid = service.as_ref().and_then(|service| service.pid);
            match cached {
                Ok(Some(listener)) if Some(listener.pid) != own_pid => Some(PortConflict {
                    port,
                    listener_pid: listener.pid,
                    listener_command: listener.command,
                }),
                _ => None,
            }
        } else {
            None
        };

        if program_exists == Some(false) {
            findings.push("program path is missing".to_string());
        }
        if !missing_paths.is_empty() {
            findings.push("a required input path is missing".to_string());
        }
        if let Some(conflict) = &port_conflict {
            findings.push(format!(
                "port {} is already held by {} (pid {})",
                conflict.port, conflict.listener_command, conflict.listener_pid
            ));
        }
        let defective =
            program_exists == Some(false) || !missing_paths.is_empty() || port_conflict.is_some();
        let running = service.as_ref().and_then(|service| service.pid).is_some();
        let last_exit = service.as_ref().and_then(|service| service.last_exit_code);
        let last_failed = last_exit.is_some_and(|code| code != 0);

        // A loop needs launchd to restart after the observed exit, no running
        // process, and a deterministic cause. Run count alone is history.
        let classification = match loaded {
            Some(true) if defective && !running && last_failed => {
                match last_exit.and_then(|code| restart_policy.restarts_after(code)) {
                    Some(true) => {
                        findings.push("launchd keeps restarting a job that cannot start".into());
                        "crash_loop"
                    }
                    None => {
                        findings.push(
                            "KeepAlive is conditional; a restart loop is not established".into(),
                        );
                        "failing"
                    }
                    Some(false) => "failing",
                }
            }
            Some(true) if defective => "failing",
            _ if disabled == Some(true) => "disabled",
            None => "unknown",
            Some(false) if defective => {
                if reloads_at_login == Some(true) && starts_at_login == Some(true) {
                    findings.push("will start at next login and fail".to_string());
                    "reload_risk"
                } else {
                    findings.push(
                        "not loaded; whether it starts at login depends on unknown overrides or conditions"
                            .to_string(),
                    );
                    "defective"
                }
            }
            _ if running => "running",
            _ if last_failed => "historic_failure",
            Some(true) => "loaded",
            Some(false) => "not_loaded",
        };
        attention |= matches!(
            classification,
            "crash_loop" | "failing" | "reload_risk" | "defective"
        );

        report.agents.push(LaunchdAgentHealth {
            label: label.to_string(),
            plist_path,
            program,
            program_exists,
            missing_paths,
            loaded,
            disabled,
            run_at_load,
            restart_policy,
            reloads_at_login,
            service,
            port_conflict,
            classification: classification.to_string(),
            findings,
        });
    }

    report.evidence_complete = report.probe_errors.is_empty();
    report.status = if attention {
        "attention"
    } else if !report.evidence_complete {
        "degraded"
    } else {
        "ok"
    }
    .to_string();
    report
}

/// Inputs that must exist for the job to start: the WorkingDirectory and,
/// for a recognized script launcher (optionally behind `env`), its script.
/// Any other absolute operand may be an output and is not inferred.
fn required_inputs<'a>(
    program: Option<&str>,
    arguments: &[&'a str],
    working_directory: Option<&'a str>,
) -> Vec<&'a str> {
    let mut required: Vec<&'a str> = working_directory
        .filter(|path| path.starts_with('/'))
        .into_iter()
        .collect();
    let base = |value: &str| {
        Path::new(value)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default()
    };
    let mut rest = arguments.get(1..).unwrap_or_default();
    let mut launcher = program.map(base).unwrap_or_default();
    if launcher == "env" {
        let skip = rest
            .iter()
            .take_while(|arg| arg.starts_with('-') || arg.contains('='))
            .count();
        rest = &rest[skip..];
        launcher = rest.first().map(|arg| base(arg)).unwrap_or_default();
        rest = rest.get(1..).unwrap_or_default();
    }
    if let Some(script) = LAUNCHERS
        .iter()
        .find(|options| options.names.contains(&launcher.as_str()))
        .and_then(|options| script_operand(options, rest))
    {
        required.push(script);
    }
    required.dedup();
    required
}

/// The script an interpreter will run, when its options establish one. Inline
/// or module code, `-` (stdin), a relative path, or any option outside the
/// known self-contained set yields `None`: uncertainty is preserved.
fn script_operand<'a>(options: &LauncherOptions, arguments: &[&'a str]) -> Option<&'a str> {
    let mut arguments = arguments.iter();
    while let Some(argument) = arguments.next() {
        let operand = if *argument == "--" {
            arguments.next().copied()?
        } else if let Some(long) = argument.strip_prefix("--") {
            let name = format!("--{}", long.split('=').next().unwrap_or_default());
            if options.inline_long.contains(&name.as_str()) {
                return None;
            }
            // `--name=value` carries its own value; a bare `--name` is safe
            // only when known not to consume the next operand.
            if long.contains('=') || options.flag_long.contains(&name.as_str()) {
                continue;
            }
            return None;
        } else if let Some(short) = argument.strip_prefix('-').filter(|short| !short.is_empty()) {
            for flag in short.chars() {
                if options.inline_short.contains(flag) || !options.flag_short.contains(flag) {
                    return None;
                }
            }
            continue;
        } else {
            argument
        };
        return operand.starts_with('/').then_some(operand);
    }
    None
}

/// Sorted `*.plist` candidates from a streamed directory listing. At most
/// `max_entries` entries are examined and `cap` candidates kept; the scan stops
/// at the first plist past the cap, the entry budget, or the deadline, and
/// each stop or unreadable entry is recorded rather than dropped.
fn collect_candidates(
    entries: impl Iterator<Item = std::io::Result<PathBuf>>,
    max_entries: usize,
    cap: usize,
    deadline: std::time::Instant,
) -> (Vec<PathBuf>, Vec<String>) {
    let mut incomplete = Vec::new();
    let mut failed = 0usize;
    let mut paths = Vec::new();
    let mut entries = entries;
    let mut examined = 0usize;
    loop {
        if std::time::Instant::now() >= deadline {
            incomplete.push("LaunchAgents scan stopped at the probe deadline".to_string());
            break;
        }
        if examined == max_entries {
            incomplete.push(format!(
                "LaunchAgents scan stopped after {max_entries} directory entries"
            ));
            break;
        }
        let Some(entry) = entries.next() else { break };
        examined += 1;
        match entry {
            Err(_) => failed += 1,
            Ok(path) if path.extension().is_some_and(|ext| ext == "plist") => {
                if paths.len() == cap {
                    incomplete.push(format!(
                        "more plists than the {cap}-file scan cap; the rest were not inspected"
                    ));
                    break;
                }
                paths.push(path);
            }
            Ok(_) => {}
        }
    }
    if failed > 0 {
        incomplete.push(format!("{failed} LaunchAgents entries could not be read"));
    }
    paths.sort();
    (paths, incomplete)
}

/// Port from `OLLAMA_HOST` (`host:port`, `:port`, or a URL), else the default.
/// Only the port is read; the environment is never reported.
fn ollama_port(plist: &Value) -> u16 {
    plist["EnvironmentVariables"]["OLLAMA_HOST"]
        .as_str()
        .and_then(|host| host.trim_end_matches('/').rsplit_once(':'))
        .and_then(|(_, port)| port.parse().ok())
        .unwrap_or(OLLAMA_DEFAULT_PORT)
}

/// Persisted overrides from `launchctl print-disabled`. Recent macOS prints
/// `=> disabled|enabled`; older releases print `=> true|false`.
pub fn parse_disabled_overrides(text: &str) -> Vec<(String, bool)> {
    text.lines()
        .filter_map(|line| {
            let (left, right) = line.split_once("=>")?;
            let label = left.trim().trim_matches('"');
            if label.is_empty() {
                return None;
            }
            let disabled = match right.trim() {
                "disabled" | "true" => true,
                "enabled" | "false" => false,
                _ => return None,
            };
            Some((label.to_string(), disabled))
        })
        .collect()
}

/// Top-level fields of `launchctl print`. Nested blocks are indented further
/// and carry their own `state =` lines, so only one-tab lines are read.
pub fn parse_service(text: &str) -> ServiceState {
    let mut state = ServiceState::default();
    for line in text.lines() {
        let Some(rest) = line.strip_prefix('\t') else {
            continue;
        };
        if rest.starts_with('\t') || rest.starts_with(' ') {
            continue;
        }
        let Some((key, value)) = rest.split_once(" = ") else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "state" if state.state.is_none() => state.state = Some(value.to_string()),
            "pid" if state.pid.is_none() => state.pid = value.parse().ok(),
            "runs" if state.runs.is_none() => state.runs = value.parse().ok(),
            "last exit code" if state.last_exit_code.is_none() => {
                state.last_exit_code = value.parse().ok()
            }
            _ => {}
        }
    }
    state
}

/// `lsof -Fpc` output: `p<pid>` then `c<command>` records.
pub fn parse_listener(text: &str) -> Option<PortListener> {
    let mut pid = None;
    let mut command = None;
    for line in text.lines() {
        if let Some(value) = line.strip_prefix('p') {
            if pid.is_some() {
                break;
            }
            pid = value.trim().parse().ok();
        } else if let Some(value) = line.strip_prefix('c') {
            command.get_or_insert_with(|| value.trim().to_string());
        }
    }
    Some(PortListener {
        pid: pid?,
        command: command.unwrap_or_default(),
    })
}

pub fn is_heiwa_label(label: &str) -> bool {
    LABEL_PREFIXES
        .iter()
        .any(|prefix| label.starts_with(prefix))
}

/// Raw result of one bounded command.
#[derive(Debug)]
pub struct ProbeOutput {
    pub status: i32,
    pub stdout: String,
    pub truncated: bool,
}

/// `launchctl print`: 0 is loaded, 113 is "no such service", anything else
/// (or a truncated listing) is a failed probe.
pub fn interpret_service(output: ProbeOutput) -> Result<Option<String>, String> {
    match output.status {
        _ if output.truncated => Err("launchctl print output truncated".to_string()),
        0 => Ok(Some(output.stdout)),
        113 => Ok(None),
        code => Err(format!("launchctl print exited {code}")),
    }
}

/// `launchctl print-disabled`: a partial override list cannot prove that a
/// label is absent, so truncation is a failure.
pub fn interpret_overrides(output: ProbeOutput) -> Result<String, String> {
    match output.status {
        _ if output.truncated => Err("override list truncated".to_string()),
        0 => Ok(output.stdout),
        code => Err(format!("launchctl print-disabled exited {code}")),
    }
}

/// `lsof -Fpc`: exit 1 with no output means no listener; truncated output or
/// any other failure is a probe error.
pub fn interpret_listener(output: ProbeOutput) -> Result<Option<PortListener>, String> {
    if output.truncated {
        return Err("lsof output truncated".to_string());
    }
    match output.status {
        1 if output.stdout.trim().is_empty() => Ok(None),
        0 => parse_listener(&output.stdout)
            .map(Some)
            .ok_or_else(|| "lsof output contains no valid listener".to_string()),
        code => Err(format!("lsof exited {code}")),
    }
}

#[cfg(target_os = "macos")]
mod system {
    use super::{
        collect_candidates, interpret_listener, interpret_overrides, interpret_service, Inventory,
        LaunchdProbe, PortListener, ProbeOutput, MAX_DIRECTORY_ENTRIES, MAX_PLISTS,
    };
    use std::io::Read;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    /// One budget for every probe in a doctor run.
    pub(crate) const TOTAL_BUDGET: Duration = Duration::from_secs(8);
    const PER_CALL: Duration = Duration::from_secs(3);
    const MAX_OUTPUT: usize = 256 * 1024;

    pub(crate) struct SystemProbe {
        agents_dir: PathBuf,
        domain: String,
        deadline: Instant,
    }

    impl SystemProbe {
        pub(crate) fn new(budget: Duration) -> Result<Self, String> {
            let deadline = Instant::now() + budget;
            let home = heiwa_config::HeiwaPaths::try_resolve()
                .map(|paths| paths.home_dir)
                .filter(|home| home.is_absolute())
                .ok_or_else(|| "home directory unavailable".to_string())?;
            let output = run("/usr/bin/id", &["-u"], deadline, MAX_OUTPUT)?;
            let uid: u32 = (output.status == 0)
                .then(|| output.stdout.trim().parse().ok())
                .flatten()
                .ok_or_else(|| "user id unavailable".to_string())?;
            Ok(Self {
                agents_dir: home.join("Library/LaunchAgents"),
                domain: format!("gui/{uid}"),
                deadline,
            })
        }
    }

    impl LaunchdProbe for SystemProbe {
        fn agent_plists(&self) -> Result<Inventory, String> {
            let entries = match std::fs::read_dir(&self.agents_dir) {
                Ok(entries) => entries,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    return Ok(Inventory::default())
                }
                Err(_) => return Err("LaunchAgents directory is unreadable".to_string()),
            };
            let (paths, incomplete) = collect_candidates(
                entries.map(|entry| entry.map(|entry| entry.path())),
                MAX_DIRECTORY_ENTRIES,
                MAX_PLISTS,
                self.deadline,
            );
            let plists = paths
                .into_iter()
                .map(|path| {
                    let parsed = path
                        .to_str()
                        .ok_or_else(|| "plist path is not UTF-8".to_string())
                        .and_then(|text| {
                            run(
                                "/usr/bin/plutil",
                                &["-convert", "json", "-o", "-", text],
                                self.deadline,
                                MAX_OUTPUT,
                            )
                        })
                        .and_then(|output| {
                            if output.status != 0 || output.truncated {
                                return Err("plist could not be read".to_string());
                            }
                            serde_json::from_str(&output.stdout)
                                .map_err(|_| "plist could not be read".to_string())
                        });
                    (path, parsed)
                })
                .collect();
            Ok(Inventory { plists, incomplete })
        }

        fn service(&self, label: &str) -> Result<Option<String>, String> {
            let target = format!("{}/{label}", self.domain);
            interpret_service(run(
                "/bin/launchctl",
                &["print", &target],
                self.deadline,
                MAX_OUTPUT,
            )?)
        }

        fn disabled_overrides(&self) -> Result<String, String> {
            interpret_overrides(run(
                "/bin/launchctl",
                &["print-disabled", &self.domain],
                self.deadline,
                MAX_OUTPUT,
            )?)
        }

        fn listener(&self, port: u16) -> Result<Option<PortListener>, String> {
            let filter = format!("-iTCP:{port}");
            let output = run(
                "/usr/sbin/lsof",
                &["-nP", &filter, "-sTCP:LISTEN", "-Fpc"],
                self.deadline,
                MAX_OUTPUT,
            )?;
            interpret_listener(output)
        }

        fn path_exists(&self, path: &Path) -> bool {
            path.exists()
        }
    }

    /// Bounded read-only command in its own process group.
    ///
    /// Guarantee: `run` returns by the smaller of `PER_CALL` and the shared
    /// deadline (plus one poll interval and SIGKILL/reap latency). Success
    /// needs both the direct child's exit and EOF on stdout; a descendant
    /// that keeps the inherited pipe open cannot extend the wait. On every
    /// return path after spawn the probe's process group receives SIGKILL,
    /// so descendants that stayed in the group end too. The reader thread is
    /// detached and finishes once the pipe's last writer is gone. Stdout past
    /// `cap` is dropped and flagged; stderr is discarded.
    pub(crate) fn run(
        program: &str,
        args: &[&str],
        deadline: Instant,
        cap: usize,
    ) -> Result<ProbeOutput, String> {
        use std::os::unix::process::CommandExt;
        use std::sync::mpsc::{sync_channel, TryRecvError};

        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("probe time budget exhausted".to_string());
        }
        let limit = Instant::now() + remaining.min(PER_CALL);
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .process_group(0)
            .spawn()
            .map_err(|_| format!("{program} could not start"))?;
        let Some(mut stdout) = child.stdout.take() else {
            release_group(&mut child);
            return Err(format!("{program} output unavailable"));
        };
        let (sender, receiver) = sync_channel(1);
        std::thread::spawn(move || {
            let mut buffer = Vec::new();
            let mut truncated = false;
            let mut chunk = [0u8; 8192];
            loop {
                match stdout.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(read) => {
                        let take = read.min(cap.saturating_sub(buffer.len()));
                        buffer.extend_from_slice(&chunk[..take]);
                        truncated |= take < read;
                    }
                }
            }
            let _ = sender.send((buffer, truncated));
        });

        let mut status = None;
        let mut output = None;
        loop {
            if output.is_none() {
                match receiver.try_recv() {
                    Ok(read) => output = Some(read),
                    Err(TryRecvError::Disconnected) => {
                        release_group(&mut child);
                        return Err(format!("{program} output unavailable"));
                    }
                    Err(TryRecvError::Empty) => {}
                }
            }
            if status.is_none() {
                match child.try_wait() {
                    Ok(next) => status = next,
                    Err(_) => {
                        release_group(&mut child);
                        return Err(format!("{program} could not be observed"));
                    }
                }
            }
            if let (Some(status), Some((stdout, truncated))) = (status, &output) {
                let output = ProbeOutput {
                    status: status.code().unwrap_or(-1),
                    stdout: String::from_utf8_lossy(stdout).into_owned(),
                    truncated: *truncated,
                };
                release_group(&mut child);
                return Ok(output);
            }
            if Instant::now() >= limit {
                release_group(&mut child);
                return Err(if status.is_some() {
                    format!("{program} timed out: output stayed open after exit")
                } else {
                    format!("{program} timed out")
                });
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// SIGKILL the probe's own process group (pgid = the child's pid, set at
    /// spawn), then reap the child. Processes that left the group via
    /// setsid/setpgid are not reached.
    fn release_group(child: &mut std::process::Child) {
        // SAFETY: killpg only signals; the pgid is the group this probe created.
        unsafe {
            libc::killpg(child.id() as libc::pid_t, libc::SIGKILL);
        }
        let _ = child.kill();
        let _ = child.wait();
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn exhausted_budget_spawns_nothing() {
            let started = Instant::now();
            let error = run("/bin/sleep", &["5"], Instant::now(), MAX_OUTPUT).unwrap_err();
            assert_eq!(error, "probe time budget exhausted");
            assert!(started.elapsed() < Duration::from_millis(100));
        }

        #[test]
        fn remaining_budget_bounds_a_slow_probe() {
            let started = Instant::now();
            let deadline = Instant::now() + Duration::from_millis(300);
            let error = run("/bin/sleep", &["5"], deadline, MAX_OUTPUT).unwrap_err();
            assert!(error.contains("timed out"), "{error}");
            assert!(started.elapsed() < Duration::from_secs(2));
        }

        /// Waits until the recorded descendant pid is gone (killed, then
        /// reaped by launchd once orphaned).
        fn assert_descendant_gone(pid_file: &Path) {
            let pid: i32 = std::fs::read_to_string(pid_file)
                .expect("descendant pid recorded")
                .trim()
                .parse()
                .expect("numeric pid");
            let started = Instant::now();
            while started.elapsed() < Duration::from_secs(2) {
                if unsafe { libc::kill(pid, 0) } != 0 {
                    return;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            unsafe { libc::kill(pid, libc::SIGKILL) };
            panic!("descendant {pid} outlived the probe");
        }

        fn pid_script(dir: &Path, script: &str) -> (PathBuf, String) {
            let pid_file = dir.join("descendant.pid");
            (
                pid_file.clone(),
                script.replace("PIDFILE", &pid_file.display().to_string()),
            )
        }

        #[test]
        fn exited_parent_with_inherited_stdout_obeys_the_deadline() {
            let dir = tempfile::tempdir().unwrap();
            let (pid_file, script) = pid_script(dir.path(), "sleep 5 & echo $! > PIDFILE");
            let started = Instant::now();
            let deadline = Instant::now() + Duration::from_millis(300);
            let result = run("/bin/sh", &["-c", &script], deadline, MAX_OUTPUT);
            assert!(
                started.elapsed() < Duration::from_secs(1),
                "{:?}",
                started.elapsed()
            );
            assert!(result.is_err(), "{result:?}");
            assert_descendant_gone(&pid_file);
        }

        #[test]
        fn timed_out_parent_with_inherited_pipe_releases_its_group() {
            let dir = tempfile::tempdir().unwrap();
            let (pid_file, script) = pid_script(dir.path(), "sleep 5 & echo $! > PIDFILE; sleep 5");
            let started = Instant::now();
            let deadline = Instant::now() + Duration::from_millis(300);
            let error = run("/bin/sh", &["-c", &script], deadline, MAX_OUTPUT).unwrap_err();
            assert!(error.contains("timed out"), "{error}");
            assert!(
                started.elapsed() < Duration::from_secs(1),
                "{:?}",
                started.elapsed()
            );
            assert_descendant_gone(&pid_file);
        }

        #[test]
        fn completed_probe_leaves_no_descendants() {
            let dir = tempfile::tempdir().unwrap();
            let (pid_file, script) = pid_script(
                dir.path(),
                "sleep 5 >/dev/null & echo $! > PIDFILE; echo ok",
            );
            let deadline = Instant::now() + Duration::from_secs(3);
            let output = run("/bin/sh", &["-c", &script], deadline, MAX_OUTPUT).unwrap();
            assert_eq!((output.status, output.stdout.as_str()), (0, "ok\n"));
            assert_descendant_gone(&pid_file);
        }

        #[test]
        fn output_closed_before_exit_is_kept() {
            let deadline = Instant::now() + Duration::from_secs(3);
            let output = run(
                "/bin/sh",
                &["-c", "echo ok; exec >&-; sleep 0.2"],
                deadline,
                MAX_OUTPUT,
            )
            .unwrap();
            assert_eq!((output.status, output.stdout.as_str()), (0, "ok\n"));
        }

        #[test]
        fn output_beyond_the_cap_is_flagged_as_truncated() {
            let deadline = Instant::now() + Duration::from_secs(5);
            let output = run("/bin/echo", &["0123456789"], deadline, 4).unwrap();
            assert!(output.truncated);
            assert_eq!(output.stdout, "0123");
            let output = run("/bin/echo", &["01"], deadline, 64).unwrap();
            assert!(!output.truncated);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::{HashMap, HashSet};

    #[derive(Default)]
    struct Fixture {
        plists: Vec<(PathBuf, Result<Value, String>)>,
        services: HashMap<String, String>,
        service_errors: HashSet<String>,
        disabled: String,
        listeners: HashMap<u16, PortListener>,
        existing: HashSet<PathBuf>,
        inventory_incomplete: Vec<String>,
        fail_listing: bool,
        fail_disabled: bool,
        fail_listener: bool,
    }

    impl LaunchdProbe for Fixture {
        fn agent_plists(&self) -> Result<Inventory, String> {
            if self.fail_listing {
                return Err("LaunchAgents directory is unreadable".to_string());
            }
            Ok(Inventory {
                plists: self.plists.clone(),
                incomplete: self.inventory_incomplete.clone(),
            })
        }
        fn service(&self, label: &str) -> Result<Option<String>, String> {
            if self.service_errors.contains(label) {
                return Err("launchctl print exited 5".to_string());
            }
            Ok(self.services.get(label).cloned())
        }
        fn disabled_overrides(&self) -> Result<String, String> {
            if self.fail_disabled {
                return Err("override list truncated".to_string());
            }
            Ok(self.disabled.clone())
        }
        fn listener(&self, port: u16) -> Result<Option<PortListener>, String> {
            if self.fail_listener {
                return Err("lsof timed out".to_string());
            }
            Ok(self.listeners.get(&port).cloned())
        }
        fn path_exists(&self, path: &Path) -> bool {
            self.existing.contains(path)
        }
    }

    impl Fixture {
        fn agent(mut self, label: &str, plist: Value) -> Self {
            self.plists.push((
                PathBuf::from(format!("/home/u/Library/LaunchAgents/{label}.plist")),
                Ok(plist),
            ));
            self
        }
        fn loaded(mut self, label: &str, text: &str) -> Self {
            self.services.insert(label.to_string(), text.to_string());
            self
        }
        fn exists(mut self, path: &str) -> Self {
            self.existing.insert(PathBuf::from(path));
            self
        }
        fn held_port(mut self, port: u16, pid: u32) -> Self {
            self.listeners.insert(
                port,
                PortListener {
                    pid,
                    command: "ollama".to_string(),
                },
            );
            self
        }
    }

    fn service_text(state: &str, pid: Option<u32>, runs: u64, last_exit: &str) -> String {
        let pid = pid
            .map(|pid| format!("\tpid = {pid}\n"))
            .unwrap_or_default();
        format!(
            "gui/501/x = {{\n\tactive count = 0\n\tstate = {state}\n\tprogram = /x\n{pid}\truns = {runs}\n\tlast exit code = {last_exit}\n\tjob state = exited\n\tendpoints = {{\n\t\tstate = active\n\t}}\n}}\n"
        )
    }

    fn orchestrator_plist(keep_alive: Value) -> Value {
        json!({
            "Label": "com.heiwa.orchestrator",
            "ProgramArguments": ["/opt/homebrew/bin/node", "/home/u/.heiwa/orchestrator/daemon.js"],
            "EnvironmentVariables": {"SECRET_TOKEN": "do-not-print"},
            "KeepAlive": keep_alive,
            "RunAtLoad": true
        })
    }

    fn ollama_plist() -> Value {
        json!({
            "Label": "com.heiwa.ollama",
            "ProgramArguments": ["/opt/homebrew/bin/ollama", "serve"],
            "KeepAlive": true,
            "RunAtLoad": true
        })
    }

    fn failing_orchestrator(keep_alive: Value) -> Fixture {
        Fixture::default()
            .agent("com.heiwa.orchestrator", orchestrator_plist(keep_alive))
            .loaded(
                "com.heiwa.orchestrator",
                &service_text("spawn scheduled", None, 4517, "1"),
            )
            .exists("/opt/homebrew/bin/node")
    }

    fn agent<'a>(report: &'a LaunchdHealthReport, label: &str) -> &'a LaunchdAgentHealth {
        report
            .agents
            .iter()
            .find(|agent| agent.label == label)
            .expect("agent reported")
    }

    #[test]
    fn missing_daemon_script_restarted_on_failure_is_a_crash_loop() {
        let report = assess(&failing_orchestrator(json!({"SuccessfulExit": false})));
        let orchestrator = agent(&report, "com.heiwa.orchestrator");
        assert_eq!(orchestrator.classification, "crash_loop");
        assert_eq!(orchestrator.restart_policy, RestartPolicy::OnFailure);
        assert_eq!(
            orchestrator.missing_paths,
            vec!["/home/u/.heiwa/orchestrator/daemon.js".to_string()]
        );
        assert_eq!(orchestrator.program_exists, Some(true));
        assert_eq!(orchestrator.reloads_at_login, Some(true));
        assert_eq!(report.status, "attention");
        assert!(report.evidence_complete);
        let serialized = serde_json::to_string(&report).unwrap();
        assert!(!serialized.contains("do-not-print"));
        assert!(!serialized.contains("SECRET_TOKEN"));
    }

    #[test]
    fn keepalive_after_success_only_does_not_loop_on_a_failed_exit() {
        let report = assess(&failing_orchestrator(json!({"SuccessfulExit": true})));
        let orchestrator = agent(&report, "com.heiwa.orchestrator");
        assert_eq!(orchestrator.restart_policy, RestartPolicy::OnSuccess);
        assert_eq!(orchestrator.classification, "failing");
        assert!(!orchestrator
            .findings
            .iter()
            .any(|finding| finding.contains("restarting")));
    }

    #[test]
    fn conditional_keepalive_does_not_claim_a_loop() {
        let report = assess(&failing_orchestrator(
            json!({"PathState": {"/tmp/x": true}, "SuccessfulExit": false}),
        ));
        let orchestrator = agent(&report, "com.heiwa.orchestrator");
        assert_eq!(orchestrator.restart_policy, RestartPolicy::Conditional);
        assert_eq!(orchestrator.classification, "failing");
        assert!(orchestrator
            .findings
            .iter()
            .any(|finding| finding.contains("not established")));
    }

    #[test]
    fn absolute_output_operands_are_not_required_inputs() {
        let fixture = Fixture::default()
            .agent(
                "ltd.heiwa.app",
                json!({
                    "Label": "ltd.heiwa.app",
                    "ProgramArguments": ["/home/u/.heiwa/bin/heiwa", "app", "start",
                        "--log", "/home/u/.heiwa/logs/new.log", "--export", "/home/u/out/report.json"],
                    "KeepAlive": true,
                    "RunAtLoad": true
                }),
            )
            .loaded(
                "ltd.heiwa.app",
                &service_text("running", Some(80860), 1, "(never exited)"),
            )
            .exists("/home/u/.heiwa/bin/heiwa");
        let report = assess(&fixture);
        let app = agent(&report, "ltd.heiwa.app");
        assert!(app.missing_paths.is_empty(), "{:?}", app.missing_paths);
        assert_eq!(app.classification, "running");
        assert_eq!(report.status, "ok");
    }

    #[test]
    fn inline_node_job_with_absolute_output_operand_is_not_a_login_failure() {
        // Reproduced by Codex: node exits 0 and never creates the operand.
        let fixture = Fixture::default()
            .agent(
                "com.heiwa.inline",
                json!({
                    "Label": "com.heiwa.inline",
                    "ProgramArguments": ["/opt/homebrew/bin/node",
                        "--eval=console.log(\"valid inline job\")", "/home/u/missing/output.txt"],
                    "RunAtLoad": true
                }),
            )
            .exists("/opt/homebrew/bin/node");
        let report = assess(&fixture);
        let inline = agent(&report, "com.heiwa.inline");
        assert!(
            inline.missing_paths.is_empty(),
            "{:?}",
            inline.missing_paths
        );
        assert_ne!(inline.classification, "reload_risk");
        assert_eq!(report.status, "ok");
    }

    #[test]
    fn plist_disabled_default_and_persisted_overrides_determine_login_loading() {
        for (default, overrides, disabled) in [
            (true, "", true),
            (true, "\"com.heiwa.default\" => enabled", false),
            (false, "\"com.heiwa.default\" => disabled", true),
        ] {
            let mut fixture = Fixture::default().agent(
                "com.heiwa.default",
                json!({
                    "Label": "com.heiwa.default",
                    "Program": "/missing/program",
                    "RunAtLoad": true,
                    "Disabled": default
                }),
            );
            fixture.disabled = overrides.to_string();
            let report = assess(&fixture);
            let job = agent(&report, "com.heiwa.default");
            assert_eq!(job.disabled, Some(disabled));
            assert_eq!(job.reloads_at_login, Some(!disabled));
            assert_eq!(
                job.classification,
                if disabled { "disabled" } else { "reload_risk" }
            );
        }
    }

    #[test]
    fn interpreter_inline_module_and_unknown_options_never_infer_a_script() {
        let none: Vec<&str> = Vec::new();
        for args in [
            vec!["/n/node", "--eval=1", "/out"],
            vec!["/n/node", "--print=1", "/out"],
            vec!["/n/node", "-e", "1", "/out"],
            vec!["/n/node", "-p1", "/out"],
            vec!["/n/node", "-r", "/hook.js", "/app.js"],
            vec!["/p/python3", "-m", "pkg", "/out"],
            vec!["/p/python3", "-cprint(1)", "/out"],
            vec!["/p/python3", "-W", "ignore", "/app.py"],
            vec!["/b/bash", "-lc", "/run", "/out"],
            vec!["/b/perl", "-e1", "/out"],
            vec!["/b/osascript", "-l", "JavaScript", "/out"],
        ] {
            assert_eq!(
                required_inputs(Some(args[0]), &args, None),
                none,
                "{args:?}"
            );
        }
        for (args, script) in [
            (
                vec!["/n/node", "--enable-source-maps", "/app.js", "/out"],
                "/app.js",
            ),
            (
                vec!["/n/node", "--max-old-space-size=512", "/app.js"],
                "/app.js",
            ),
            (vec!["/p/python3", "-uB", "/app.py", "/out"], "/app.py"),
            (vec!["/b/bash", "-e", "/run.sh"], "/run.sh"),
            (vec!["/n/node", "--", "/app.js"], "/app.js"),
        ] {
            assert_eq!(
                required_inputs(Some(args[0]), &args, None),
                vec![script],
                "{args:?}"
            );
        }
    }

    #[test]
    fn heiwa_label_in_a_custom_filename_is_assessed_and_others_are_counted() {
        let mut fixture = Fixture::default().exists("/x");
        fixture.plists.push((
            PathBuf::from("/home/u/Library/LaunchAgents/custom-agent.plist"),
            Ok(json!({"Label": "com.heiwa.foo", "Program": "/x"})),
        ));
        fixture.plists.push((
            PathBuf::from("/home/u/Library/LaunchAgents/homebrew.mxcl.ollama.plist"),
            Ok(json!({"Label": "homebrew.mxcl.ollama", "Program": "/opt/homebrew/bin/ollama"})),
        ));
        let report = assess(&fixture);
        assert_eq!(agent(&report, "com.heiwa.foo").classification, "not_loaded");
        assert_eq!(report.other_plists, 1);
        assert_eq!(report.status, "ok");
    }

    #[test]
    fn incomplete_inventory_and_agent_cap_degrade_the_report() {
        let report = assess(&Fixture {
            inventory_incomplete: vec!["1 LaunchAgents entry unreadable".to_string()],
            ..Fixture::default()
        });
        assert_eq!(report.status, "degraded");
        assert!(!report.evidence_complete);

        let mut capped = Fixture::default().exists("/x");
        for index in 0..MAX_AGENTS + 2 {
            capped.plists.push((
                PathBuf::from(format!("/home/u/Library/LaunchAgents/a{index}.plist")),
                Ok(json!({"Label": format!("com.heiwa.a{index}"), "Program": "/x"})),
            ));
        }
        let report = assess(&capped);
        assert_eq!(report.agents.len(), MAX_AGENTS);
        assert_eq!(report.status, "degraded");
        assert!(report
            .probe_errors
            .iter()
            .any(|error| error.contains("2 Heiwa agents")));
    }

    #[test]
    fn candidate_collection_stops_without_consuming_the_whole_directory() {
        let later = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let consumed = std::cell::Cell::new(0usize);
        let entries = (0..100_000).map(|index| {
            consumed.set(consumed.get() + 1);
            Ok(PathBuf::from(format!("/la/{index}.plist")))
        });
        let (paths, incomplete) = collect_candidates(entries, 1_000, 3, later);
        assert_eq!(paths.len(), 3);
        assert_eq!(consumed.get(), 4, "stops at the first plist past the cap");
        assert_eq!(incomplete.len(), 1, "{incomplete:?}");

        consumed.set(0);
        let others = (0..100_000).map(|index| {
            consumed.set(consumed.get() + 1);
            Ok(PathBuf::from(format!("/la/{index}.txt")))
        });
        let (paths, incomplete) = collect_candidates(others, 50, 3, later);
        assert!(paths.is_empty());
        assert_eq!(consumed.get(), 50, "entry budget bounds non-plist scanning");
        assert_eq!(incomplete.len(), 1, "{incomplete:?}");

        consumed.set(0);
        let expired = std::time::Instant::now();
        let entries = (0..100_000).map(|index| {
            consumed.set(consumed.get() + 1);
            Ok(PathBuf::from(format!("/la/{index}.plist")))
        });
        let (paths, incomplete) = collect_candidates(entries, 1_000, 256, expired);
        assert!(paths.is_empty());
        assert_eq!(consumed.get(), 0, "an expired deadline inspects nothing");
        assert!(incomplete[0].contains("deadline"), "{incomplete:?}");
    }

    #[test]
    fn truncated_or_failed_lsof_output_is_a_probe_error() {
        let output = |status, stdout: &str, truncated| ProbeOutput {
            status,
            stdout: stdout.to_string(),
            truncated,
        };
        assert!(interpret_listener(output(0, "p80861\ncollama\n", true)).is_err());
        assert!(interpret_listener(output(1, "", true)).is_err());
        assert!(interpret_listener(output(2, "", false)).is_err());
        for status in [-1, -9, 0] {
            assert!(interpret_listener(output(status, "", false)).is_err());
        }
        assert!(interpret_listener(output(0, "unrecognized output\n", false)).is_err());
        assert!(interpret_listener(output(1, "p80861\ncollama\n", false)).is_err());
        assert_eq!(interpret_listener(output(1, "", false)), Ok(None));
        assert_eq!(
            interpret_listener(output(0, "p80861\ncollama\n", false)),
            Ok(Some(PortListener {
                pid: 80861,
                command: "ollama".to_string()
            }))
        );
    }

    #[test]
    fn candidate_collection_counts_entry_errors_and_scan_cap() {
        let entries = vec![
            Ok(PathBuf::from("/la/b.plist")),
            Err(std::io::Error::other("bad entry")),
            Ok(PathBuf::from("/la/notes.txt")),
            Ok(PathBuf::from("/la/a.plist")),
            Ok(PathBuf::from("/la/c.plist")),
        ];
        let later = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let (paths, incomplete) = collect_candidates(entries.into_iter(), 100, 2, later);
        assert_eq!(
            paths,
            vec![PathBuf::from("/la/a.plist"), PathBuf::from("/la/b.plist")]
        );
        assert_eq!(incomplete.len(), 2, "{incomplete:?}");
    }

    #[test]
    fn required_inputs_cover_launchers_env_and_working_directory() {
        assert_eq!(
            required_inputs(
                Some("/usr/bin/env"),
                &[
                    "/usr/bin/env",
                    "PYTHONUNBUFFERED=1",
                    "python3",
                    "-u",
                    "/srv/job.py",
                    "/srv/out.txt"
                ],
                Some("/srv"),
            ),
            vec!["/srv", "/srv/job.py"]
        );
        assert!(required_inputs(Some("/bin/sh"), &["/bin/sh", "-c", "/srv/run"], None).is_empty());
        assert!(required_inputs(
            Some("/opt/homebrew/bin/ollama"),
            &["/opt/homebrew/bin/ollama", "serve", "/srv/models"],
            None
        )
        .is_empty());
    }

    #[test]
    fn duplicate_ollama_supervisor_on_held_port_is_a_crash_loop() {
        let fixture = Fixture::default()
            .agent("com.heiwa.ollama", ollama_plist())
            .loaded(
                "com.heiwa.ollama",
                &service_text("spawn scheduled", None, 27032, "1"),
            )
            .exists("/opt/homebrew/bin/ollama")
            .held_port(OLLAMA_DEFAULT_PORT, 80861);
        let report = assess(&fixture);
        let ollama = agent(&report, "com.heiwa.ollama");
        assert_eq!(ollama.classification, "crash_loop");
        assert_eq!(
            ollama.port_conflict,
            Some(PortConflict {
                port: 11434,
                listener_pid: 80861,
                listener_command: "ollama".to_string(),
            })
        );
        assert_eq!(report.status, "attention");
    }

    #[test]
    fn ollama_host_port_override_is_respected() {
        let mut plist = ollama_plist();
        plist["EnvironmentVariables"] = json!({"OLLAMA_HOST": "127.0.0.1:11500"});
        let fixture = Fixture::default()
            .agent("com.heiwa.ollama", plist)
            .loaded(
                "com.heiwa.ollama",
                &service_text("running", Some(42), 1, "(never exited)"),
            )
            .exists("/opt/homebrew/bin/ollama")
            .held_port(OLLAMA_DEFAULT_PORT, 80861);
        let report = assess(&fixture);
        let ollama = agent(&report, "com.heiwa.ollama");
        assert_eq!(ollama.port_conflict, None);
        assert_eq!(ollama.classification, "running");
    }

    #[test]
    fn disabled_retained_plists_do_not_warn_and_external_ollama_is_preserved() {
        let mut fixture = Fixture::default()
            .agent(
                "com.heiwa.orchestrator",
                orchestrator_plist(json!({"SuccessfulExit": false})),
            )
            .agent("com.heiwa.ollama", ollama_plist())
            .exists("/opt/homebrew/bin/node")
            .exists("/opt/homebrew/bin/ollama")
            .held_port(OLLAMA_DEFAULT_PORT, 80861);
        fixture.disabled = "disabled services = {\n\t\t\"com.heiwa.orchestrator\" => disabled\n\t\t\"com.heiwa.ollama\" => disabled\n\t\t\"homebrew.mxcl.ollama\" => enabled\n\t}\n".to_string();
        let report = assess(&fixture);
        for label in ["com.heiwa.orchestrator", "com.heiwa.ollama"] {
            let entry = agent(&report, label);
            assert_eq!(entry.classification, "disabled", "{label}");
            assert_eq!(entry.disabled, Some(true));
            assert_eq!(entry.reloads_at_login, Some(false));
            assert!(entry.port_conflict.is_none());
        }
        assert_eq!(report.status, "ok");
        assert!(report
            .agents
            .iter()
            .all(|entry| is_heiwa_label(&entry.label)));
    }

    #[test]
    fn retained_enabled_plist_with_defect_is_a_login_reload_risk() {
        let fixture = Fixture::default()
            .agent(
                "com.heiwa.orchestrator",
                orchestrator_plist(json!({"SuccessfulExit": false})),
            )
            .exists("/opt/homebrew/bin/node");
        let report = assess(&fixture);
        let orchestrator = agent(&report, "com.heiwa.orchestrator");
        assert_eq!(orchestrator.loaded, Some(false));
        assert_eq!(orchestrator.reloads_at_login, Some(true));
        assert_eq!(orchestrator.classification, "reload_risk");
        assert_eq!(report.status, "attention");
    }

    #[test]
    fn conditional_start_without_runatload_is_not_a_definite_login_failure() {
        let fixture = Fixture::default()
            .agent(
                "com.heiwa.orchestrator",
                json!({
                    "Label": "com.heiwa.orchestrator",
                    "ProgramArguments": ["/opt/homebrew/bin/node", "/home/u/.heiwa/orchestrator/daemon.js"],
                    "KeepAlive": {"PathState": {"/tmp/x": true}}
                }),
            )
            .exists("/opt/homebrew/bin/node");
        let report = assess(&fixture);
        let orchestrator = agent(&report, "com.heiwa.orchestrator");
        assert_eq!(orchestrator.classification, "defective");
        assert!(!orchestrator
            .findings
            .iter()
            .any(|finding| finding.contains("will start")));
    }

    #[test]
    fn historic_nonzero_exit_without_loop_evidence_is_not_a_loop() {
        let fixture = Fixture::default()
            .agent(
                "ltd.heiwa.ea.brief",
                json!({
                    "Label": "ltd.heiwa.ea.brief",
                    "ProgramArguments": ["/home/u/.heiwa/bin/heiwa", "life", "brief"],
                    "StartCalendarInterval": {"Hour": 7}
                }),
            )
            .loaded(
                "ltd.heiwa.ea.brief",
                &service_text("not running", None, 900, "1"),
            )
            .exists("/home/u/.heiwa/bin/heiwa");
        let report = assess(&fixture);
        let brief = agent(&report, "ltd.heiwa.ea.brief");
        assert_eq!(brief.classification, "historic_failure");
        assert_eq!(report.status, "ok");
    }

    #[test]
    fn healthy_running_agent_is_ok() {
        let fixture = Fixture::default()
            .agent(
                "ltd.heiwa.app",
                json!({
                    "Label": "ltd.heiwa.app",
                    "ProgramArguments": ["/home/u/.heiwa/bin/heiwa", "app", "start", "--port", "7474"],
                    "KeepAlive": true,
                    "RunAtLoad": true
                }),
            )
            .loaded(
                "ltd.heiwa.app",
                &service_text("running", Some(80860), 1, "(never exited)"),
            )
            .exists("/home/u/.heiwa/bin/heiwa");
        let report = assess(&fixture);
        let app = agent(&report, "ltd.heiwa.app");
        assert_eq!(app.classification, "running");
        let service = app.service.as_ref().expect("service state");
        assert_eq!(service.pid, Some(80860));
        assert_eq!(service.last_exit_code, None);
        assert_eq!(service.state.as_deref(), Some("running"));
        assert_eq!(report.status, "ok");
        assert!(report.evidence_complete);
    }

    #[test]
    fn failed_probes_degrade_the_report_instead_of_reading_ok() {
        let unavailable = assess(&Fixture {
            fail_listing: true,
            ..Fixture::default()
        });
        assert_eq!(unavailable.status, "unavailable");
        assert!(!unavailable.evidence_complete);

        let mut partial = Fixture::default()
            .agent("com.heiwa.ollama", ollama_plist())
            .loaded(
                "com.heiwa.ollama",
                &service_text("running", Some(7), 1, "(never exited)"),
            )
            .exists("/opt/homebrew/bin/ollama");
        partial.fail_disabled = true;
        partial.fail_listener = true;
        let report = assess(&partial);
        let ollama = agent(&report, "com.heiwa.ollama");
        assert_eq!(ollama.disabled, None);
        assert_eq!(ollama.reloads_at_login, None);
        assert_eq!(ollama.port_conflict, None);
        assert_eq!(report.status, "degraded");
        assert!(!report.evidence_complete);
        assert_eq!(report.probe_errors.len(), 2);
    }

    #[test]
    fn failed_service_probe_is_unknown_not_unloaded() {
        let mut fixture = Fixture::default()
            .agent(
                "ltd.heiwa.app",
                json!({"Label": "ltd.heiwa.app", "Program": "/x"}),
            )
            .exists("/x");
        fixture.service_errors.insert("ltd.heiwa.app".to_string());
        let report = assess(&fixture);
        let app = agent(&report, "ltd.heiwa.app");
        assert_eq!(app.loaded, None);
        assert_eq!(app.classification, "unknown");
        assert_eq!(report.status, "degraded");
    }

    #[test]
    fn launchctl_exit_codes_and_truncation_are_interpreted_strictly() {
        let output = |status, truncated| ProbeOutput {
            status,
            stdout: "x".to_string(),
            truncated,
        };
        assert_eq!(
            interpret_service(output(0, false)),
            Ok(Some("x".to_string()))
        );
        assert_eq!(interpret_service(output(113, false)), Ok(None));
        assert!(interpret_service(output(5, false)).is_err());
        assert!(interpret_service(output(0, true)).is_err());
        assert_eq!(interpret_overrides(output(0, false)), Ok("x".to_string()));
        assert!(interpret_overrides(output(0, true)).is_err());
        assert!(interpret_overrides(output(1, false)).is_err());
    }

    #[test]
    fn unreadable_plists_degrade_and_foreign_labels_are_only_counted() {
        let mut fixture = Fixture::default().agent(
            "com.heiwa.mislabeled",
            json!({"Label": "homebrew.mxcl.ollama", "Program": "/opt/homebrew/bin/ollama"}),
        );
        fixture.plists.push((
            PathBuf::from("/home/u/Library/LaunchAgents/com.heiwa.broken.plist"),
            Err("plist could not be read".to_string()),
        ));
        let report = assess(&fixture);
        assert!(report.agents.is_empty());
        assert_eq!(report.other_plists, 1);
        assert_eq!(report.probe_errors.len(), 1);
        assert!(report.probe_errors[0].contains("ownership unknown"));
        assert!(!serde_json::to_string(&report).unwrap().contains("homebrew"));
        assert_eq!(report.status, "degraded");
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn non_macos_reports_unsupported() {
        let report = check_launchd_health();
        assert_eq!(report.status, "unsupported");
        assert!(report.agents.is_empty());
    }

    #[test]
    fn parsers_handle_real_formats() {
        let overrides =
            parse_disabled_overrides("\t\t\"com.heiwa.ollama\" => disabled\n\t\t\"x\" => false\n");
        assert_eq!(
            overrides,
            vec![
                ("com.heiwa.ollama".to_string(), true),
                ("x".to_string(), false)
            ]
        );
        assert_eq!(
            parse_listener("p80861\ncollama\nf3\n"),
            Some(PortListener {
                pid: 80861,
                command: "ollama".to_string()
            })
        );
        assert_eq!(parse_listener(""), None);
    }
}
