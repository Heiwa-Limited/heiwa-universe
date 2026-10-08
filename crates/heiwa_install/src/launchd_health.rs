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
/// Interpreters whose first non-flag operand is the script they run.
const SCRIPT_LAUNCHERS: [&str; 12] = [
    "node",
    "nodejs",
    "bun",
    "python",
    "python3",
    "ruby",
    "perl",
    "php",
    "bash",
    "sh",
    "zsh",
    "osascript",
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
    /// Plists in scope whose label is not Heiwa-owned.
    pub skipped: Vec<String>,
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
            skipped: Vec::new(),
            probe_errors: Vec::new(),
        }
    }

    pub fn needs_attention(&self) -> bool {
        self.status == "attention"
    }
}

/// Each candidate plist path with its JSON form, or why it could not be read.
pub type AgentPlists = Vec<(PathBuf, Result<Value, String>)>;

/// Read-only sources for the assessment. The system probe shells out to
/// `plutil`, `launchctl`, and `lsof` under one time budget; tests substitute
/// fixtures. An `Err` means the evidence is missing or incomplete.
pub trait LaunchdProbe {
    fn agent_plists(&self) -> Result<AgentPlists, String>;
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
    let plists = match probe.agent_plists() {
        Ok(plists) => plists,
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
    let mut listener_cache: Vec<(u16, Result<Option<PortListener>, String>)> = Vec::new();
    let mut attention = false;

    for (plist_path, parsed) in plists.into_iter().take(MAX_AGENTS) {
        let name = plist_path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let plist = match parsed {
            Ok(plist) => plist,
            Err(error) => {
                report.probe_errors.push(format!("{name}: {error}"));
                continue;
            }
        };
        let Some(label) = plist["Label"]
            .as_str()
            .filter(|label| is_heiwa_label(label))
        else {
            report.skipped.push(name);
            continue;
        };

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
                .any(|(name, disabled)| name == label && *disabled)
        });
        // A plist in LaunchAgents loads at login unless a persisted disable
        // override exists. Unknown overrides leave the answer unknown.
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
    if SCRIPT_LAUNCHERS.contains(&launcher.as_str()) {
        if let Some(script) = rest.iter().find(|arg| !arg.starts_with('-')) {
            if script.starts_with('/') && !rest.iter().any(|arg| *arg == "-c" || *arg == "-e") {
                required.push(script);
            }
        }
    }
    required.dedup();
    required
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

#[cfg(target_os = "macos")]
mod system {
    use super::{
        interpret_overrides, interpret_service, is_heiwa_label, parse_listener, AgentPlists,
        LaunchdProbe, PortListener, ProbeOutput, MAX_AGENTS,
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
        fn agent_plists(&self) -> Result<AgentPlists, String> {
            let entries = match std::fs::read_dir(&self.agents_dir) {
                Ok(entries) => entries,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
                Err(_) => return Err("LaunchAgents directory is unreadable".to_string()),
            };
            let mut paths: Vec<PathBuf> = entries
                .filter_map(|entry| entry.ok().map(|entry| entry.path()))
                .filter(|path| {
                    path.extension().is_some_and(|ext| ext == "plist")
                        && path
                            .file_stem()
                            .and_then(|stem| stem.to_str())
                            .is_some_and(is_heiwa_label)
                })
                .collect();
            paths.sort();
            paths.truncate(MAX_AGENTS);
            Ok(paths
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
                .collect())
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
            // lsof exits 1 with no output when nothing matches.
            match (output.status, output.stdout.trim().is_empty()) {
                (_, true) if output.status <= 1 => Ok(None),
                (0, false) => Ok(parse_listener(&output.stdout)),
                (code, _) => Err(format!("lsof exited {code}")),
            }
        }

        fn path_exists(&self, path: &Path) -> bool {
            path.exists()
        }
    }

    /// Bounded read-only command. Each call gets the smaller of the per-call
    /// limit and what remains of the shared deadline; stdout beyond `cap` is
    /// dropped and flagged; stderr is discarded.
    pub(crate) fn run(
        program: &str,
        args: &[&str],
        deadline: Instant,
        cap: usize,
    ) -> Result<ProbeOutput, String> {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("probe time budget exhausted".to_string());
        }
        let timeout = remaining.min(PER_CALL);
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| format!("{program} could not start"))?;
        let mut stdout = child
            .stdout
            .take()
            .ok_or_else(|| format!("{program} output unavailable"))?;
        let reader = std::thread::spawn(move || {
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
            (buffer, truncated)
        });
        let started = Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if started.elapsed() < timeout => {
                    std::thread::sleep(Duration::from_millis(10))
                }
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = reader.join();
                    return Err(format!("{program} timed out"));
                }
            }
        };
        let (output, truncated) = reader.join().unwrap_or_default();
        Ok(ProbeOutput {
            status: status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output).into_owned(),
            truncated,
        })
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
        fail_listing: bool,
        fail_disabled: bool,
        fail_listener: bool,
    }

    impl LaunchdProbe for Fixture {
        fn agent_plists(&self) -> Result<AgentPlists, String> {
            if self.fail_listing {
                return Err("LaunchAgents directory is unreadable".to_string());
            }
            Ok(self.plists.clone())
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
    fn unreadable_plists_degrade_and_foreign_labels_are_skipped() {
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
        assert_eq!(
            report.skipped,
            vec!["com.heiwa.mislabeled.plist".to_string()]
        );
        assert_eq!(report.probe_errors.len(), 1);
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
