use heiwa_session::{start_daemon, get_session_dir};
use std::fs;
use tokio::net::UnixStream;
use std::time::Duration;

#[tokio::test]
async fn test_session_daemon_socket_creation() {
    let session_dir = get_session_dir();
    if session_dir.exists() {
        fs::remove_dir_all(&session_dir).ok();
    }
    
    let info = start_daemon().expect("failed to start daemon");
    
    // Check socket exists with retry
    let mut retry = 0;
    while !info.socket_path.exists() && retry < 10 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        retry += 1;
    }
    assert!(info.socket_path.exists(), "Socket file should be created at {:?}", info.socket_path);
    
    // Try to connect
    let mut retry = 0;
    let stream = loop {
        match UnixStream::connect(&info.socket_path).await {
            Ok(s) => break Ok(s),
            Err(e) if retry < 5 => {
                tokio::time::sleep(Duration::from_millis(100)).await;
                retry += 1;
                continue;
            }
            Err(e) => break Err(e),
        }
    };
    
    assert!(stream.is_ok(), "Should be able to connect to the session socket");
}
