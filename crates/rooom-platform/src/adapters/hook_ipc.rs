use super::*;
use crate::adapters::{claude, storage};
use rooom_core::inbox::Decision;
use std::{
    os::windows::ffi::OsStringExt,
    path::{Path, PathBuf},
};
use windows_sys::Win32::System::{
    Pipes::GetNamedPipeClientProcessId, Threading::QueryFullProcessImageNameW,
};

pub const MAX_PAYLOAD: usize = 64 * 1024;

pub fn output(decision: Decision) -> serde_json::Value {
    rooom_clients::claude::hook_output(decision == Decision::Allow)
}

fn endpoint(scope: &Scope) -> Scope {
    Scope {
        sid: scope.sid.clone(),
        session: scope.session,
        name: format!("{}-hook", scope.name),
    }
}

fn timeout_ms() -> Duration {
    let milliseconds = std::env::var("ROOOM_FIXTURE_HOOK_TIMEOUT_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| (100..=600_000).contains(v))
        .unwrap_or(600_000);
    Duration::from_millis(milliseconds)
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

pub fn listen(scope: &Scope, root: PathBuf) -> io::Result<Listener> {
    let scope = endpoint(scope);
    let Reservation {
        runtime,
        mut server,
        ..
    } = Reservation::bind(&scope)?;
    let (stop, mut stopped) = oneshot::channel();
    let thread = thread::Builder::new()
        .name("rooom-hook-pipe".into())
        .spawn(move || {
            runtime.block_on(async {
                loop {
                    let connected = tokio::select! {
                        _ = &mut stopped => return,
                        connected = server.connect() => connected,
                    };
                    match connected {
                        Ok(()) => {
                            tokio::select! {
                                _ = &mut stopped => return,
                                _ = exchange(&mut server, &scope, &root) => {},
                            }
                        }
                        Err(err) if err.raw_os_error() == Some(ERROR_NO_DATA as i32) => {}
                        Err(_) => return,
                    }
                    let _ = server.disconnect();
                }
            });
        })?;
    Ok(Listener {
        stop: Some(stop),
        thread: Some(thread),
    })
}

async fn exchange(server: &mut NamedPipeServer, scope: &Scope, root: &Path) -> io::Result<()> {
    let mut peer_pid = 0;
    checked(unsafe { GetNamedPipeClientProcessId(server.as_raw_handle(), &mut peer_pid) })?;
    scope.verify_process(peer_pid)?;
    storage::verify_private(root)?;
    let caller = claude::verify_hook_peer(root, peer_pid).map_err(invalid)?;
    let request = timeout(EXCHANGE, async {
        let count = server.read_u32_le().await? as usize;
        if count == 0 || count > MAX_PAYLOAD {
            return Err(invalid("invalid hook frame size"));
        }
        let mut payload = vec![0; count];
        server.read_exact(&mut payload).await?;
        claude::register_hook(root, now_ms(), caller, &payload).map_err(invalid)
    })
    .await
    .map_err(|_| io::ErrorKind::TimedOut)??;
    let key = request.key;
    let result = timeout(timeout_ms(), async {
        loop {
            if let Some(answer) = claude::take_hook_answer(root, now_ms(), &key) {
                server
                    .write_u8(match answer {
                        Decision::Allow => 1,
                        Decision::Deny => 2,
                        Decision::AllowForSession => return Err(invalid("Unsupported hook decision")),
                    })
                    .await?;
                if timeout(EXCHANGE, server.read_u8())
                    .await
                    .map_err(|_| io::ErrorKind::TimedOut)??
                    != ACK
                {
                    return Err(invalid("invalid hook acknowledgement"));
                }
                if !claude::confirm_hook_answer(root, &key) {
                    return Err(invalid("hook answer was not pending"));
                }
                return Ok(());
            }
            tokio::select! {
                _ = sleep(Duration::from_millis(40)) => {}
                _ = server.read_u8() => return Err(io::ErrorKind::BrokenPipe.into()),
            }
        }
    })
    .await
    .map_err(|_| io::Error::from(io::ErrorKind::TimedOut))
    .and_then(|r| r);
    if result.is_err() {
        claude::cancel_hook(root, &key);
    }
    result
}

pub fn decide(
    scope: &Scope,
    payload: &[u8],
    reply: impl FnOnce(Decision) -> io::Result<()>,
) -> io::Result<()> {
    if payload.is_empty() || payload.len() > MAX_PAYLOAD {
        return Err(invalid("invalid hook frame size"));
    }
    let scope = endpoint(scope);
    runtime()?.block_on(async {
        let mut client = timeout(EXCHANGE, async {
            loop {
                match ClientOptions::new().open(scope.name()) {
                    Ok(client) => return Ok(client),
                    Err(error) if error.raw_os_error() == Some(ERROR_PIPE_BUSY as i32) => {
                        sleep(Duration::from_millis(40)).await;
                    }
                    Err(error) => return Err(error),
                }
            }
        })
        .await
        .map_err(|_| io::ErrorKind::TimedOut)??;
        let mut server_pid = 0;
        checked(unsafe { GetNamedPipeServerProcessId(client.as_raw_handle(), &mut server_pid) })?;
        scope.verify_process(server_pid)?;
        if std::fs::canonicalize(image(server_pid)?)?
            != std::fs::canonicalize(std::env::current_exe()?)?
        {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        timeout(EXCHANGE, async {
            client.write_u32_le(payload.len() as u32).await?;
            client.write_all(payload).await
        })
        .await
        .map_err(|_| io::ErrorKind::TimedOut)??;
        let response = timeout(timeout_ms() + EXCHANGE, client.read_u8())
            .await
            .map_err(|_| io::ErrorKind::TimedOut)??;
        let decision = match response {
            1 => Decision::Allow,
            2 => Decision::Deny,
            _ => return Err(invalid("invalid hook decision")),
        };
        reply(decision)?;
        client.write_u8(ACK).await?;
        Ok(())
    })
}

fn image(pid: u32) -> io::Result<PathBuf> {
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        return Err(io::Error::last_os_error());
    }
    let process = OwnedHandle(process);
    let mut name = vec![0u16; 32_768];
    let mut length = name.len() as u32;
    checked(unsafe { QueryFullProcessImageNameW(process.0, 0, name.as_mut_ptr(), &mut length) })?;
    Ok(std::ffi::OsString::from_wide(&name[..length as usize]).into())
}
