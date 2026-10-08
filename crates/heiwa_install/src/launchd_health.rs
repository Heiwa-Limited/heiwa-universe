//! Report-only health of Heiwa-owned launchd agents.
//!
//! Doctor reads `~/Library/LaunchAgents` plists whose label starts with
//! `com.heiwa.` or `ltd.heiwa.`, the matching `launchctl` service state, the
//! persisted disable overrides, and the listener on a supervised local port.
//! Nothing here mutates launchd, files, or provider state. Reports carry the
//! program path and missing paths only: no other argv, environment, or logs.

use serde::Serialize;
use serde_json::Value;
use std::path::{Path, PathBuf};

const LABEL_PREFIXES: [&str; 2] = ["com.heiwa.", "ltd.heiwa."];
const MAX_AGENTS: usize = 32;
const OLLAMA_DEFAULT_PORT: u16 = 11434;

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

#[derive(Debug, Clone, Serialize)]
pub struct LaunchdAgentHealth {
    pub label: String,
    pub plist_path: PathBuf,
    pub program: Option<String>,
    /// `None` when the program is not an absolute path and cannot be checked.
    pub program_exists: Option<bool>,
    pub missing_paths: Vec<String>,
    pub loaded: bool,
    pub disabled: Option<bool>,
    pub run_at_load: bool,
    pub keep_alive: bool,
    pub reloads_at_login: Option<bool>,
    pub service: Option<ServiceState>,
    pub port_conflict: Option<PortConflict>,
    pub classification: String,
    pub findings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LaunchdHealthReport {
    /// `ok`, `attention`, `unsupported`, or `unavailable`.
    pub status: String,
    pub platform: String,
    pub scope: String,
    pub agents: Vec<LaunchdAgentHealth>,
    pub probe_errors: Vec<String>,
}

impl LaunchdHealthReport {
    fn empty(status: &str) -> Self {
        Self {
            status: status.to_string(),
            platform: std::env::consts::OS.to_string(),
            scope: "user LaunchAgents with com.heiwa.* or ltd.heiwa.* labels".to_string(),
            agents: Vec::new(),
            probe_errors: Vec::new(),
        }
    }

    pub fn needs_attention(&self) -> bool {
        self.status == "attention"
    }
}

/// Read-only sources for the assessment. The system probe shells out to
/// `plutil`, `launchctl`, and `lsof`; tests substitute fixtures.
/// Each candidate plist path with its JSON form, or why it could not be read.
pub type AgentPlists = Vec<(PathBuf, Result<Value, String>)>;

pub trait LaunchdProbe {
    /// Plist path and its JSON form for each candidate agent file.
    fn agent_plists(&self) -> Result<AgentPlists, String>;
    /// `launchctl print` text for a loaded label; `Ok(None)` when not loaded.
    fn service(&self, label: &str) -> Result<Option<String>, String>;
    /// `launchctl print-disabled` text for the user domain.
    fn disabled_overrides(&self) -> Result<String, String>;
    fn listener(&self, port: u16) -> Result<Option<PortListener>, String>;
    fn path_exists(&self, path: &Path) -> bool;
}

pub fn check_launchd_health() -> LaunchdHealthReport {
    #[cfg(target_os = "macos")]
    {
        match system::SystemProbe::new() {
            Some(probe) => assess(&probe),
            None => {
                let mut report = LaunchdHealthReport::empty("unavailable");
                report
                    .probe_errors
                    .push("home directory or user id unavailable".to_string());
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
            report.probe_errors.push(error);
            return report;
        }
    };
    let overrides = match probe.disabled_overrides() {
        Ok(text) => Some(parse_disabled_overrides(&text)),
        Err(error) => {
            report.probe_errors.push(error);
            None
        }
    };
    let mut listener_cache: Vec<(u16, Result<Option<PortListener>, String>)> = Vec::new();

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
            report
                .probe_errors
                .push(format!("{name}: label is not Heiwa-owned; skipped"));
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
        // Absolute operands (scripts, configs) and the working directory must
        // exist for the job to start. Other argv stays out of the report.
        let mut missing_paths: Vec<String> = arguments
            .iter()
            .skip(1)
            .chain(plist["WorkingDirectory"].as_str().iter())
            .filter(|value| value.starts_with('/') && !probe.path_exists(Path::new(value)))
            .map(|value| value.to_string())
            .collect();
        missing_paths.dedup();
        let run_at_load = plist["RunAtLoad"].as_bool().unwrap_or(false);
        let keep_alive = match &plist["KeepAlive"] {
            Value::Bool(value) => *value,
            Value::Object(conditions) => !conditions.is_empty(),
            _ => false,
        };
        let disabled = overrides.as_ref().map(|overrides| {
            overrides
                .iter()
                .any(|(name, disabled)| name == label && *disabled)
        });

        let mut findings = Vec::new();
        let (loaded, service) = match probe.service(label) {
            Ok(Some(text)) => (true, Some(parse_service(&text))),
            Ok(None) => (false, None),
            Err(error) => {
                report.probe_errors.push(format!("{label}: {error}"));
                (false, None)
            }
        };
        // A plist in LaunchAgents loads at login unless a persisted disable
        // override exists. Unknown overrides leave the answer unknown.
        let reloads_at_login = disabled.map(|disabled| !disabled);

        let port_conflict = if program
            .as_deref()
            .and_then(|program| Path::new(program).file_name())
            .is_some_and(|name| name == "ollama")
            && (loaded || reloads_at_login == Some(true))
        {
            let port = ollama_port(&plist);
            let cached = match listener_cache.iter().find(|(cached, _)| *cached == port) {
                Some((_, result)) => result.clone(),
                None => {
                    let result = probe.listener(port);
                    listener_cache.push((port, result.clone()));
                    if let Err(error) = &result {
                        report.probe_errors.push(format!("port {port}: {error}"));
                    }
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
            findings.push("an absolute path the job needs is missing".to_string());
        }
        if let Some(conflict) = &port_conflict {
            findings.push(format!(
                "port {} is already held by {} (pid {})",
                conflict.port, conflict.listener_command, conflict.listener_pid
            ));
        }
        let defective = !findings.is_empty();
        let running = service.as_ref().and_then(|service| service.pid).is_some();
        let last_failed = service
            .as_ref()
            .and_then(|service| service.last_exit_code)
            .is_some_and(|code| code != 0);

        // A loop needs a live restart policy, no running process, a failed
        // last exit, and a deterministic cause. Run count alone is history.
        let classification = if loaded && !running && keep_alive && last_failed && defective {
            findings.push("launchd keeps restarting a job that cannot start".to_string());
            "crash_loop"
        } else if loaded && defective {
            "failing"
        } else if disabled == Some(true) {
            "disabled"
        } else if !loaded
            && defective
            && reloads_at_login == Some(true)
            && (run_at_load || keep_alive)
        {
            findings.push("will start at next login and fail".to_string());
            "reload_risk"
        } else if running {
            "running"
        } else if last_failed {
            "historic_failure"
        } else if loaded {
            "loaded"
        } else {
            "not_loaded"
        };
        if matches!(classification, "crash_loop" | "failing" | "reload_risk") {
            report.status = "attention".to_string();
        }

        report.agents.push(LaunchdAgentHealth {
            label: label.to_string(),
            plist_path,
            program,
            program_exists,
            missing_paths,
            loaded,
            disabled,
            run_at_load,
            keep_alive,
            reloads_at_login,
            service,
            port_conflict,
            classification: classification.to_string(),
            findings,
        });
    }
    report
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

#[cfg(target_os = "macos")]
mod system {
    use super::{
        is_heiwa_label, parse_listener, AgentPlists, LaunchdProbe, PortListener, MAX_AGENTS,
    };
    use std::io::Read;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    const TIMEOUT: Duration = Duration::from_secs(3);
    const MAX_OUTPUT: usize = 256 * 1024;

    pub(super) struct SystemProbe {
        agents_dir: PathBuf,
        domain: String,
    }

    impl SystemProbe {
        pub(super) fn new() -> Option<Self> {
            let home = std::env::var_os("HOME").filter(|value| !value.is_empty())?;
            let (status, uid) = run("/usr/bin/id", &["-u"]).ok()?;
            let uid: u32 = (status == 0).then(|| uid.trim().parse().ok())??;
            Some(Self {
                agents_dir: PathBuf::from(home).join("Library/LaunchAgents"),
                domain: format!("gui/{uid}"),
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
                            run("/usr/bin/plutil", &["-convert", "json", "-o", "-", text])
                        })
                        .and_then(|(status, output)| {
                            if status != 0 {
                                return Err("plist could not be read".to_string());
                            }
                            serde_json::from_str(&output)
                                .map_err(|_| "plist could not be read".to_string())
                        });
                    (path, parsed)
                })
                .collect())
        }

        fn service(&self, label: &str) -> Result<Option<String>, String> {
            let target = format!("{}/{label}", self.domain);
            let (status, output) = run("/bin/launchctl", &["print", &target])?;
            // 113: launchd has no such service in this domain.
            Ok((status == 0).then_some(output))
        }

        fn disabled_overrides(&self) -> Result<String, String> {
            let (status, output) = run("/bin/launchctl", &["print-disabled", &self.domain])?;
            if status != 0 {
                return Err("launchctl print-disabled failed".to_string());
            }
            Ok(output)
        }

        fn listener(&self, port: u16) -> Result<Option<PortListener>, String> {
            let filter = format!("-iTCP:{port}");
            let (status, output) =
                run("/usr/sbin/lsof", &["-nP", &filter, "-sTCP:LISTEN", "-Fpc"])?;
            // lsof exits 1 when nothing matches.
            if status != 0 && output.trim().is_empty() {
                return Ok(None);
            }
            Ok(parse_listener(&output))
        }

        fn path_exists(&self, path: &Path) -> bool {
            path.exists()
        }
    }

    /// Bounded read-only command: fixed timeout and output cap, stderr dropped.
    fn run(program: &str, args: &[&str]) -> Result<(i32, String), String> {
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
            let mut chunk = [0u8; 8192];
            loop {
                match stdout.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(read) => {
                        if buffer.len() < MAX_OUTPUT {
                            let take = read.min(MAX_OUTPUT - buffer.len());
                            buffer.extend_from_slice(&chunk[..take]);
                        }
                    }
                }
            }
            buffer
        });
        let started = Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if started.elapsed() < TIMEOUT => {
                    std::thread::sleep(Duration::from_millis(20))
                }
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = reader.join();
                    return Err(format!("{program} timed out"));
                }
            }
        };
        let output = reader.join().unwrap_or_default();
        Ok((
            status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output).into_owned(),
        ))
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
            Ok(self.services.get(label).cloned())
        }
        fn disabled_overrides(&self) -> Result<String, String> {
            if self.fail_disabled {
                return Err("launchctl print-disabled failed".to_string());
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
    }

    fn service_text(state: &str, pid: Option<u32>, runs: u64, last_exit: &str) -> String {
        let pid = pid
            .map(|pid| format!("\tpid = {pid}\n"))
            .unwrap_or_default();
        format!(
            "gui/501/x = {{\n\tactive count = 0\n\tstate = {state}\n\tprogram = /x\n{pid}\truns = {runs}\n\tlast exit code = {last_exit}\n\tjob state = exited\n\tendpoints = {{\n\t\tstate = active\n\t}}\n}}\n"
        )
    }

    fn orchestrator_plist() -> Value {
        json!({
            "Label": "com.heiwa.orchestrator",
            "ProgramArguments": ["/opt/homebrew/bin/node", "/home/u/.heiwa/orchestrator/daemon.js"],
            "EnvironmentVariables": {"SECRET_TOKEN": "do-not-print"},
            "KeepAlive": {"SuccessfulExit": false},
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

    fn agent<'a>(report: &'a LaunchdHealthReport, label: &str) -> &'a LaunchdAgentHealth {
        report
            .agents
            .iter()
            .find(|agent| agent.label == label)
            .expect("agent reported")
    }

    #[test]
    fn missing_daemon_script_under_keepalive_is_a_crash_loop() {
        let fixture = Fixture::default()
            .agent("com.heiwa.orchestrator", orchestrator_plist())
            .loaded(
                "com.heiwa.orchestrator",
                &service_text("spawn scheduled", None, 4517, "1"),
            )
            .exists("/opt/homebrew/bin/node");
        let report = assess(&fixture);
        let orchestrator = agent(&report, "com.heiwa.orchestrator");
        assert_eq!(orchestrator.classification, "crash_loop");
        assert_eq!(
            orchestrator.missing_paths,
            vec!["/home/u/.heiwa/orchestrator/daemon.js".to_string()]
        );
        assert_eq!(orchestrator.program_exists, Some(true));
        assert_eq!(orchestrator.reloads_at_login, Some(true));
        assert_eq!(report.status, "attention");
        let serialized = serde_json::to_string(&report).unwrap();
        assert!(!serialized.contains("do-not-print"));
        assert!(!serialized.contains("SECRET_TOKEN"));
    }

    #[test]
    fn duplicate_ollama_supervisor_on_held_port_is_a_crash_loop() {
        let mut fixture = Fixture::default()
            .agent("com.heiwa.ollama", ollama_plist())
            .loaded(
                "com.heiwa.ollama",
                &service_text("spawn scheduled", None, 27032, "1"),
            )
            .exists("/opt/homebrew/bin/ollama");
        fixture.listeners.insert(
            OLLAMA_DEFAULT_PORT,
            PortListener {
                pid: 80861,
                command: "ollama".to_string(),
            },
        );
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
        let mut fixture = Fixture::default()
            .agent("com.heiwa.ollama", plist)
            .loaded(
                "com.heiwa.ollama",
                &service_text("running", Some(42), 1, "(never exited)"),
            )
            .exists("/opt/homebrew/bin/ollama");
        fixture.listeners.insert(
            OLLAMA_DEFAULT_PORT,
            PortListener {
                pid: 80861,
                command: "ollama".to_string(),
            },
        );
        let report = assess(&fixture);
        let ollama = agent(&report, "com.heiwa.ollama");
        assert_eq!(ollama.port_conflict, None);
        assert_eq!(ollama.classification, "running");
    }

    #[test]
    fn disabled_retained_plists_do_not_warn_and_external_ollama_is_preserved() {
        let mut fixture = Fixture::default()
            .agent("com.heiwa.orchestrator", orchestrator_plist())
            .agent("com.heiwa.ollama", ollama_plist())
            .exists("/opt/homebrew/bin/node")
            .exists("/opt/homebrew/bin/ollama");
        fixture.disabled = "disabled services = {\n\t\t\"com.heiwa.orchestrator\" => disabled\n\t\t\"com.heiwa.ollama\" => disabled\n\t\t\"homebrew.mxcl.ollama\" => enabled\n\t}\n".to_string();
        fixture.listeners.insert(
            OLLAMA_DEFAULT_PORT,
            PortListener {
                pid: 80861,
                command: "ollama".to_string(),
            },
        );
        let report = assess(&fixture);
        for label in ["com.heiwa.orchestrator", "com.heiwa.ollama"] {
            let entry = agent(&report, label);
            assert_eq!(entry.classification, "disabled", "{label}");
            assert_eq!(entry.disabled, Some(true));
            assert_eq!(entry.reloads_at_login, Some(false));
            assert!(entry.port_conflict.is_none());
        }
        assert_eq!(report.status, "ok");
        // Only Heiwa-owned labels are reported.
        assert!(report
            .agents
            .iter()
            .all(|entry| is_heiwa_label(&entry.label)));
    }

    #[test]
    fn retained_enabled_plist_with_defect_is_a_login_reload_risk() {
        let fixture = Fixture::default()
            .agent("com.heiwa.orchestrator", orchestrator_plist())
            .exists("/opt/homebrew/bin/node");
        let report = assess(&fixture);
        let orchestrator = agent(&report, "com.heiwa.orchestrator");
        assert!(!orchestrator.loaded);
        assert_eq!(orchestrator.reloads_at_login, Some(true));
        assert_eq!(orchestrator.classification, "reload_risk");
        assert_eq!(report.status, "attention");
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
    }

    #[test]
    fn unavailable_probes_degrade_without_failing() {
        let fixture = Fixture {
            fail_listing: true,
            ..Fixture::default()
        };
        let report = assess(&fixture);
        assert_eq!(report.status, "unavailable");
        assert!(!report.probe_errors.is_empty());

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
        assert_eq!(ollama.classification, "running");
        assert_eq!(report.status, "ok");
        assert_eq!(report.probe_errors.len(), 2);

        // Unknown overrides leave an unloaded agent's port unprobed.
        let mut unknown = Fixture::default()
            .agent("com.heiwa.ollama", ollama_plist())
            .exists("/opt/homebrew/bin/ollama");
        unknown.fail_disabled = true;
        unknown.fail_listener = true;
        let report = assess(&unknown);
        assert_eq!(
            agent(&report, "com.heiwa.ollama").classification,
            "not_loaded"
        );
        assert_eq!(report.probe_errors.len(), 1);
    }

    #[test]
    fn unreadable_or_foreign_plists_are_reported_or_skipped() {
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
        assert_eq!(report.probe_errors.len(), 2);
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
