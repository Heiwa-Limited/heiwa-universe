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
    command
        .stdin(Stdio::null())
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
    loop {
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
        if status.is_some() && output.is_some() {
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
