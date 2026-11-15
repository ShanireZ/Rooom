#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use rooom_platform::pipe::{Action, Reservation, Scope};
    use std::{
        env, fs, io,
        path::Path,
        thread,
        time::{Duration, Instant},
    };
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeClient};
    use tokio::time::{sleep, timeout};

    fn wait_for(path: &Path) -> io::Result<String> {
        let limit = Instant::now() + Duration::from_secs(75);
        loop {
            if let Ok(text) = fs::read_to_string(path) {
                return Ok(text);
            }
            if Instant::now() >= limit {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    path.display().to_string(),
                ));
            }
            thread::sleep(Duration::from_millis(120));
        }
    }

    fn write(path: &Path, value: impl AsRef<[u8]>) -> io::Result<()> {
        let temp = path.with_extension("tmp");
        fs::write(&temp, value)?;
        fs::rename(temp, path)
    }

    async fn open(name: &str) -> io::Result<NamedPipeClient> {
        loop {
            match ClientOptions::new().open(name) {
                Err(error) if error.raw_os_error() == Some(231) => {
                    sleep(Duration::from_millis(40)).await
                }
                other => return other,
            }
        }
    }

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()?;
    let args: Vec<String> = env::args().collect();
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        [_, "denied", name, output] => {
            let code = runtime
                .block_on(async { timeout(Duration::from_secs(6), open(name)).await })?
                .err()
                .and_then(|error| error.raw_os_error())
                .unwrap_or(0);
            write(Path::new(output), code.to_string())?;
            if code != 5 {
                return Err(
                    io::Error::other("foreign account was not denied by the pipe ACL").into(),
                );
            }
        }
        [_, "resident", directory, role @ ("console" | "peer")] => {
            let directory = Path::new(directory);
            let other = if *role == "console" {
                "peer"
            } else {
                "console"
            };
            let scope = Scope::current()?;
            let _listener = Reservation::bind(&scope)?.serve(
                || None,
                |error| {
                    eprintln!("Rooom fixture listener failed: {error}");
                    std::process::exit(1);
                },
            )?;
            write(&directory.join(format!("{role}.ready")), scope.name())?;
            let foreign = wait_for(&directory.join(format!("{other}.ready")))?;
            let health = scope.request(Action::Health)?;
            if health.pid != std::process::id() || health.session != scope.session() {
                return Err(
                    io::Error::other("health did not reach the resident in this session").into(),
                );
            }
            let rejected = runtime.block_on(async {
                let mut client = timeout(Duration::from_secs(6), open(&foreign)).await??;
                timeout(Duration::from_secs(3), async {
                    if client.write_all(&[1, 1]).await.is_err() {
                        return Ok::<_, io::Error>(true);
                    }
                    let mut response = [0u8; 1];
                    Ok(matches!(client.read(&mut response).await, Ok(0) | Err(_)))
                })
                .await?
            })?;
            if !rejected {
                return Err(io::Error::other("foreign Windows session received a reply").into());
            }
            // Keep both real endpoints alive for the duration of the peer and ACL checks.
            write(
                &directory.join(format!("{role}.result")),
                format!("{},{}", health.pid, health.session),
            )?;
            wait_for(&directory.join("release"))?;
        }
        _ => {
            return Err(
                io::Error::other("expected a resident or denied fixture invocation").into(),
            );
        }
    }
    Ok(())
}

#[cfg(not(windows))]
fn main() {}
