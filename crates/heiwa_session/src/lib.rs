use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::PathBuf;
use tokio::net::UnixListener;
use uuid::Uuid;
use portable_pty::{native_pty_system, PtySize, CommandBuilder};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub session_id: String,
    pub socket_path: PathBuf,
}

pub fn get_session_dir() -> PathBuf {
    let home = env::var("HOME")
        .or_else(|_| env::var("USERPROFILE"))
        .expect("HOME or USERPROFILE must be set");
    PathBuf::from(home).join(".heiwa").join("sessions")
}

pub fn start_daemon() -> Result<SessionInfo> {
    let session_id = Uuid::new_v4().to_string();
    let session_dir = get_session_dir();
    fs::create_dir_all(&session_dir)?;
    
    let socket_path = session_dir.join(format!("{}.sock", session_id));
    
    // Spawn the daemon task
    let socket_path_clone = socket_path.clone();
    tokio::spawn(async move {
        let listener = UnixListener::bind(&socket_path_clone).expect("failed to bind socket");
        loop {
            match listener.accept().await {
                Ok((_stream, _addr)) => {
                    // Handle control connection
                }
                Err(e) => eprintln!("Accept error: {}", e),
            }
        }
    });

    Ok(SessionInfo {
        session_id,
        socket_path,
    })
}

pub fn attach_session(_session_id: &str) -> Result<()> {
    // This will eventually be used by the CLI to talk to the socket
    Ok(())
}

pub struct PtySession {
    pub master: Box<dyn portable_pty::MasterPty + Send>,
}

impl PtySession {
    pub fn new() -> Result<Self> {
        let pty_system = native_pty_system();
        let pair = pty_system.openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let shell = if cfg!(target_os = "windows") {
            "cmd.exe"
        } else {
            "bash"
        };
        
        let cmd = CommandBuilder::new(shell);
        let _child = pair.slave.spawn_command(cmd)?;

        Ok(Self {
            master: pair.master,
        })
    }
}
