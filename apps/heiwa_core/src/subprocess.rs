//! Bounded subprocess output for native resource reads. Stdout is protocol data;
//! stderr stays private and is never forwarded into the product UI.
use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub fn bounded_output(
    command: &mut Command,
    timeout: Duration,
    max_bytes: usize,
) -> Result<Vec<u8>, String> {
    bounded_output_inner(command, None, timeout, max_bytes)
}

/// Run a bounded protocol subprocess with request bytes on stdin. Input and
/// output are pumped concurrently so neither pipe can block the other.
pub fn bounded_output_with_input(
    command: &mut Command,
    input: &[u8],
    timeout: Duration,
    max_bytes: usize,
) -> Result<Vec<u8>, String> {
    bounded_output_inner(command, Some(input), timeout, max_bytes)
}

const MAX_INPUT_BYTES: usize = 4 * 1024 * 1024;

fn bounded_output_inner(
    command: &mut Command,
    input: Option<&[u8]>,
    timeout: Duration,
    max_bytes: usize,
) -> Result<Vec<u8>, String> {
    if input.is_some_and(|bytes| bytes.len() > MAX_INPUT_BYTES) {
        return Err("The local resource request is too large.".to_string());
    }
    command
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // The CLI launches osascript. A separate process group lets timeout
        // cleanup stop both rather than leaving an Automation read behind.
        unsafe {
            command.pre_exec(|| {
                if libc::setpgid(0, 0) == 0 {
                    Ok(())
                } else {
                    Err(std::io::Error::last_os_error())
                }
            });
        }
    }

    let mut child = command
        .spawn()
        .map_err(|_| "Local resource reading could not start.".to_string())?;
    let stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => {
            terminate_process_tree(&mut child);
            return Err("Local resource reading could not capture its result.".to_string());
        }
    };
    let (input_tx, input_rx) = std::sync::mpsc::sync_channel(1);
    if let Some(input) = input {
        let mut stdin = child.stdin.take().ok_or_else(|| {
            terminate_process_tree(&mut child);
            "Local resource reading could not send its request.".to_string()
        })?;
        let input = input.to_vec();
        std::thread::spawn(move || {
            use std::io::Write;
            let result = stdin.write_all(&input);
            let _ = input_tx.send(result.is_ok());
        });
    } else {
        let _ = input_tx.send(true);
    }
    let (output_tx, output_rx) = std::sync::mpsc::sync_channel(1);
    let _reader = std::thread::spawn(move || {
        let mut stdout = stdout;
        let mut bytes = Vec::new();
        let mut chunk = [0_u8; 4096];
        loop {
            match stdout.read(&mut chunk) {
                Ok(0) => {
                    let _ = output_tx.send(Ok(bytes));
                    return;
                }
                Ok(count) if bytes.len() + count <= max_bytes => {
                    bytes.extend_from_slice(&chunk[..count]);
                }
                Ok(_) => {
                    let _ = output_tx.send(Err(
                        "The Heiwa runtime returned an oversized local resource result."
                            .to_string(),
                    ));
                    return;
                }
                Err(_) => {
                    let _ = output_tx.send(Err(
                        "Local resource reading could not capture its result.".to_string(),
                    ));
                    return;
                }
            }
        }
    });
    let started = Instant::now();
    let mut status = None;
    let mut output = None;
    let mut input_written = false;
    loop {
        if !input_written {
            match input_rx.try_recv() {
                Ok(true) => input_written = true,
                Ok(false) => {
                    terminate_process_tree(&mut child);
                    return Err("Local resource reading could not send its request.".to_string());
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    terminate_process_tree(&mut child);
                    return Err("Local resource reading could not send its request.".to_string());
                }
                Err(_) => {}
            }
        }
        match output_rx.try_recv() {
            Ok(Ok(bytes)) => output = Some(bytes),
            Ok(Err(error)) => {
                terminate_process_tree(&mut child);
                return Err(error);
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) if output.is_none() => {
                terminate_process_tree(&mut child);
                return Err("Local resource reading could not capture its result.".to_string());
            }
            Err(_) => {}
        }
        match child.try_wait() {
            Ok(Some(next)) => status = Some(next),
            Ok(None) => {}
            Err(_) => {
                terminate_process_tree(&mut child);
                return Err("Local resource reading stopped unexpectedly.".to_string());
            }
        }
        if status.is_some() && output.is_some() && input_written {
            break;
        }
        if started.elapsed() >= timeout {
            terminate_process_tree(&mut child);
            return Err(
                "The local resource did not respond before the read timed out.".to_string(),
            );
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    let status = status.expect("checked above");
    let output = output.expect("checked above");
    if !status.success() {
        return Err(
            "The local resource could not be read. Check its permissions in System Settings."
                .to_string(),
        );
    }
    Ok(output)
}

fn terminate_process_tree(child: &mut std::process::Child) {
    #[cfg(unix)]
    unsafe {
        libc::killpg(child.id() as libc::pid_t, libc::SIGKILL);
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// Capture a governed command's stdout and stderr in a caller-owned private
/// spool. Unlike native protocol reads, nonzero exits are returned as data so
/// the operator can diagnose and repair work. Bounds apply while it runs.
pub fn bounded_command_output(
    command: &mut Command,
    spool: &mut std::fs::File,
    timeout: Duration,
    max_bytes: usize,
) -> Result<(std::process::ExitStatus, Vec<u8>), String> {
    use std::io::{Seek, SeekFrom};
    command
        .stdin(Stdio::null())
        .stdout(Stdio::from(spool.try_clone().map_err(|e| e.to_string())?))
        .stderr(Stdio::from(spool.try_clone().map_err(|e| e.to_string())?));
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        unsafe {
            command.pre_exec(|| {
                if libc::setpgid(0, 0) == 0 {
                    Ok(())
                } else {
                    Err(std::io::Error::last_os_error())
                }
            });
        }
    }
    let mut child = command.spawn().map_err(|e| e.to_string())?;
    let start = Instant::now();
    let status = loop {
        let size = match spool.metadata() {
            Ok(metadata) => metadata.len(),
            Err(error) => {
                terminate_process_tree(&mut child);
                return Err(error.to_string());
            }
        };
        if size > max_bytes as u64 {
            terminate_process_tree(&mut child);
            return Err("Command output limit exceeded".into());
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(error) => {
                terminate_process_tree(&mut child);
                return Err(error.to_string());
            }
        }
        if start.elapsed() >= timeout {
            terminate_process_tree(&mut child);
            return Err("Command timed out; its process group was stopped".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    // Background children must not outlive the command receipt.
    terminate_process_tree(&mut child);
    spool.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    spool
        .take(max_bytes as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > max_bytes {
        return Err("Command output limit exceeded".into());
    }
    Ok((status, bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn input_and_output_are_pumped_together_without_argv_payload() {
        let input = format!(
            "{{\"private\":\"PRIVATE_SENTINEL_{}\"}}",
            "x".repeat(2 * 1024 * 1024)
        );
        let mut command = Command::new("python3");
        command.args(["-c", "import sys; assert sys.argv[1:] == []; sys.stdout.write('x'*131072); sys.stdout.flush(); data=sys.stdin.read(); sys.stdout.write(data)"]);
        let output = bounded_output_with_input(
            &mut command,
            input.as_bytes(),
            Duration::from_secs(5),
            3 * 1024 * 1024,
        )
        .expect("bounded request/response");
        assert_eq!(&output[131_072..], input.as_bytes());
    }

    #[cfg(unix)]
    #[test]
    fn early_exit_and_timeout_return_without_hanging_on_input() {
        let input = vec![b'x'; 1024 * 1024];
        let mut early = Command::new("python3");
        early.args(["-c", "import sys; sys.exit(0)"]);
        assert!(
            bounded_output_with_input(&mut early, &input, Duration::from_secs(2), 1024).is_err()
        );

        let mut slow = Command::new("python3");
        slow.args(["-c", "import sys,time; sys.stdin.read(); time.sleep(5)"]);
        let result = bounded_output_with_input(&mut slow, &input, Duration::from_millis(100), 1024);
        assert_eq!(
            result.unwrap_err(),
            "The local resource did not respond before the read timed out."
        );
    }
}
