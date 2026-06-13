use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique identifier for an automation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AutomationId(pub Uuid);

impl AutomationId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn from_string(s: &str) -> anyhow::Result<Self> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl std::fmt::Display for AutomationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::str::FromStr for AutomationId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl Default for AutomationId {
    fn default() -> Self {
        Self::new()
    }
}

/// Unique identifier for an execution
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ExecutionId(pub Uuid);

impl ExecutionId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl std::fmt::Display for ExecutionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Automation trigger configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TriggerConfig {
    Cron(CronTriggerConfig),
    FileWatch(FileWatchTriggerConfig),
}

/// Cron trigger configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CronTriggerConfig {
    /// Cron expression (e.g., "0 9 * * 1" for Monday 9am)
    pub schedule: String,
    /// Timezone (e.g., "America/New_York"). Defaults to system timezone.
    pub timezone: Option<String>,
}

/// File watch trigger configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileWatchTriggerConfig {
    /// Paths to watch (supports ~ for home directory)
    pub paths: Vec<String>,
    /// Events to trigger on
    pub events: Vec<FileWatchEvent>,
    /// Optional file pattern filter (glob, e.g., "*.pdf")
    pub pattern: Option<String>,
    /// Debounce in milliseconds (prevents rapid firing). Default: 500
    pub debounce_ms: Option<u64>,
}

/// File watch event type
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FileWatchEvent {
    Create,
    Modify,
    Delete,
}

/// Trigger event data passed when trigger fires
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TriggerEventData {
    Cron {
        timestamp: DateTime<Utc>,
        scheduled_time: DateTime<Utc>,
    },
    FileWatch {
        timestamp: DateTime<Utc>,
        file: FileWatchEventData,
    },
    External {
        timestamp: DateTime<Utc>,
        source: ExternalSource,
        metadata: Option<serde_json::Value>,
    },
}

/// External trigger source
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ExternalSource {
    Api,
    Script,
    Webhook,
}

/// File watch event data
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileWatchEventData {
    pub path: String,
    pub event: FileWatchEvent,
    pub size: Option<u64>,
}

/// Automation status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutomationStatus {
    Active,
    Paused,
    Disabled,
}

/// Automation definition
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Automation {
    pub id: AutomationId,
    pub name: String,
    pub description: Option<String>,
    pub prompt: String,
    pub trigger_config: Option<TriggerConfig>,
    pub status: AutomationStatus,
    pub max_iterations: u32,
    pub max_executions_per_day: Option<u32>,
    pub max_executions_per_hour: Option<u32>,
    pub last_executed_at: Option<DateTime<Utc>>,
    pub next_scheduled_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Execution status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStatus {
    Pending,
    Running,
    AwaitingConfirmation,
    Completed,
    Failed,
    Cancelled,
}

/// Execution record
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Execution {
    pub id: ExecutionId,
    pub automation_id: AutomationId,
    pub status: ExecutionStatus,
    pub trigger_data: Option<TriggerEventData>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub error_message: Option<String>,
    pub retry_count: u32,
    pub created_at: DateTime<Utc>,
}

/// Pending confirmation status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PendingConfirmationStatus {
    Pending,
    Approved,
    Denied,
    Expired,
}

impl Automation {
    pub fn new(name: String, prompt: String) -> Self {
        let now = Utc::now();
        Self {
            id: AutomationId::new(),
            name,
            description: None,
            prompt,
            trigger_config: None,
            status: AutomationStatus::Disabled,
            max_iterations: 0, // 0 = unlimited
            max_executions_per_day: None,
            max_executions_per_hour: None,
            last_executed_at: None,
            next_scheduled_at: None,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn with_cron_trigger(mut self, schedule: String, timezone: Option<String>) -> Self {
        self.trigger_config = Some(TriggerConfig::Cron(CronTriggerConfig {
            schedule,
            timezone,
        }));
        self
    }

    pub fn with_file_watch_trigger(mut self, config: FileWatchTriggerConfig) -> Self {
        self.trigger_config = Some(TriggerConfig::FileWatch(config));
        self
    }

    pub fn activate(mut self) -> Self {
        self.status = AutomationStatus::Active;
        self.updated_at = Utc::now();
        self
    }
}

impl FileWatchTriggerConfig {
    pub fn new(paths: Vec<String>, events: Vec<FileWatchEvent>) -> Self {
        Self {
            paths,
            events,
            pattern: None,
            debounce_ms: Some(500),
        }
    }
}

impl Default for FileWatchTriggerConfig {
    fn default() -> Self {
        Self::new(vec![], vec![])
    }
}
