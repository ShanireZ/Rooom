use std::ffi::c_void;
use std::io;
use std::os::windows::io::AsRawHandle;
use std::ptr::null_mut;
use std::thread::{self, JoinHandle};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::windows::named_pipe::{
    ClientOptions, NamedPipeClient, NamedPipeServer, ServerOptions,
};
use tokio::runtime::{Builder, Runtime};
use tokio::sync::oneshot;
use tokio::time::{Instant, sleep, timeout};
use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_ACCESS_DENIED, ERROR_FILE_NOT_FOUND, ERROR_NO_DATA, ERROR_PIPE_BUSY,
    ERROR_PIPE_NOT_CONNECTED, HANDLE, LocalFree,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
};
use windows_sys::Win32::Security::{
    GetTokenInformation, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER, TokenUser,
};
use windows_sys::Win32::System::Pipes::{
    GetNamedPipeClientSessionId, GetNamedPipeServerProcessId, GetNamedPipeServerSessionId,
};
use windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
};

const VERSION: u8 = 1;
const ACK: u8 = 6;
const EXCHANGE: Duration = Duration::from_millis(1500);
const STARTUP: Duration = Duration::from_secs(10);

#[derive(Debug)]
pub struct Scope {
    sid: String,
    session: u32,
    name: String,
}

impl Scope {
    pub fn current() -> io::Result<Self> {
        let sid = process_sid(unsafe { GetCurrentProcess() })?;
        let mut session = 0;
        checked(unsafe { ProcessIdToSessionId(std::process::id(), &mut session) })?;
        let name = format!(r"\\.\pipe\Rooom-{sid}-{session}");
        Ok(Self { sid, session, name })
    }

    // Disposable probes use independent names without changing OS identities.
    pub fn fixture(mut self, suffix: &str) -> io::Result<Self> {
        if suffix.is_empty()
            || !suffix
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err(invalid("invalid fixture suffix"));
        }
        self.name.push_str("-fixture-");
        self.name.push_str(suffix);
        Ok(self)
    }

    pub fn session(&self) -> u32 {
        self.session
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn request(&self, action: Action) -> io::Result<Receipt> {
        let runtime = runtime()?;
        runtime.block_on(async {
            timeout(STARTUP, self.request_when_ready(action))
                .await
                .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "Rooom did not reply"))?
        })
    }

    async fn request_when_ready(&self, action: Action) -> io::Result<Receipt> {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            match ClientOptions::new().open(&self.name) {
                Ok(client) => return self.exchange(client, action).await,
                Err(error) if error.raw_os_error() == Some(ERROR_PIPE_BUSY as i32) => {}
                Err(error)
                    if error.raw_os_error() == Some(ERROR_FILE_NOT_FOUND as i32)
                        && Instant::now() < deadline => {}
                Err(error) => return Err(error),
            }
            sleep(Duration::from_millis(40)).await;
        }
    }

    async fn exchange(&self, mut client: NamedPipeClient, action: Action) -> io::Result<Receipt> {
        let mut pid = 0;
        let mut session = 0;
        checked(unsafe { GetNamedPipeServerProcessId(client.as_raw_handle(), &mut pid) })?;
        checked(unsafe { GetNamedPipeServerSessionId(client.as_raw_handle(), &mut session) })?;
        if session != self.session {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "foreign Windows session",
            ));
        }
        let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if process.is_null() {
            return Err(io::Error::last_os_error());
        }
        let process = OwnedHandle(process);
        if process_sid(process.0)? != self.sid {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "foreign pipe owner",
            ));
        }
        client.write_all(&[VERSION, action as u8]).await?;
        let mut reply = [0; 10];
        client.read_exact(&mut reply).await?;
        if reply != response(action, pid, session) {
            return Err(invalid("invalid Rooom reply"));
        }
        client.write_all(&[ACK]).await?;
        Ok(Receipt { pid, session })
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Action {
    Health = 1,
    Activate = 2,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Receipt {
    pub pid: u32,
    pub session: u32,
}

pub struct Reservation {
    runtime: Runtime,
    server: NamedPipeServer,
    session: u32,
}

pub enum Startup {
    Primary(Reservation),
    Activated(Receipt),
}

pub fn start_or_activate(scope: &Scope) -> io::Result<Startup> {
    match Reservation::bind(scope) {
        Ok(reservation) => Ok(Startup::Primary(reservation)),
        Err(error) if name_taken(&error) => scope.request(Action::Activate).map(Startup::Activated),
        Err(error) => Err(error),
    }
}

fn name_taken(error: &io::Error) -> bool {
    matches!(error.raw_os_error(), Some(code)
        if code == ERROR_ACCESS_DENIED as i32 || code == ERROR_PIPE_BUSY as i32)
}

impl Reservation {
    pub fn bind(scope: &Scope) -> io::Result<Self> {
        let runtime = runtime()?;
        let descriptor_text: Vec<u16> = format!("D:P(A;;GA;;;{})\0", scope.sid)
            .encode_utf16()
            .collect();
        let mut descriptor = null_mut();
        checked(unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                descriptor_text.as_ptr(),
                1,
                &mut descriptor,
                null_mut(),
            )
        })?;
        let descriptor = LocalAllocation(descriptor);
        let mut security = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor.0,
            bInheritHandle: 0,
        };
        let server = {
            let _reactor = runtime.enter();
            // The descriptor stays alive through CreateNamedPipe; the kernel copies it.
            unsafe {
                ServerOptions::new()
                    .first_pipe_instance(true)
                    .reject_remote_clients(true)
                    .max_instances(1)
                    .create_with_security_attributes_raw(
                        &scope.name,
                        (&mut security as *mut SECURITY_ATTRIBUTES).cast(),
                    )?
            }
        };
        Ok(Self {
            runtime,
            server,
            session: scope.session,
        })
    }

    /// The callback finishes only after the resident accepts the panel transition.
    pub fn serve(
        self,
        activate: impl Fn() -> Option<oneshot::Receiver<()>> + Send + 'static,
        on_failure: impl FnOnce(io::Error) + Send + 'static,
    ) -> io::Result<Listener> {
        let (stop, mut stopped) = oneshot::channel();
        let Self {
            runtime,
            mut server,
            session,
        } = self;
        let thread = thread::Builder::new().name("rooom-pipe".into()).spawn(move || {
            runtime.block_on(async {
                let failure = tokio::select! {
                    _ = &mut stopped => None,
                    failure = async {
                        loop {
                            match server.connect().await {
                                Ok(()) => {
                                    let _ = timeout(EXCHANGE, serve_client(&mut server, session, &activate)).await;
                                }
                                Err(error) if error.raw_os_error() == Some(ERROR_NO_DATA as i32) => {}
                                Err(error) => return error,
                            }
                            if let Err(error) = server.disconnect() {
                                // The peer can already be gone by the time the response is acknowledged.
                                if error.raw_os_error() != Some(ERROR_PIPE_NOT_CONNECTED as i32) {
                                    return error;
                                }
                            }
                        }
                    } => Some(failure),
                };
                if let Some(failure) = failure {
                    on_failure(failure);
                    // Hold the first instance until its owning process is done.
                    let _ = stopped.await;
                }
            });
        })?;
        Ok(Listener {
            stop: Some(stop),
            thread: Some(thread),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    #[test]
    fn reserves_one_listener_and_routes_activation_only_after_accepting_it() {
        let scope = Scope::current()
            .unwrap()
            .fixture(&format!("handover-{}", std::process::id()))
            .unwrap();
        let Startup::Primary(primary) = start_or_activate(&scope).unwrap() else {
            panic!("first instance was not primary");
        };
        let error = Reservation::bind(&scope)
            .err()
            .expect("second bind succeeded");
        assert!(name_taken(&error), "unexpected second bind error: {error}");
        let activations = Arc::new(AtomicUsize::new(0));
        let observed = activations.clone();
        let _listener = primary
            .serve(
                move || {
                    let (done, receiver) = activation_reply();
                    observed.fetch_add(1, Ordering::SeqCst);
                    done.send(()).unwrap();
                    Some(receiver)
                },
                |error| panic!("listener failed: {error}"),
            )
            .unwrap();
        let health = scope.request(Action::Health).unwrap();
        assert_eq!(
            health,
            Receipt {
                pid: std::process::id(),
                session: scope.session()
            }
        );
        assert_eq!(activations.load(Ordering::SeqCst), 0);
        let Startup::Activated(activated) = start_or_activate(&scope).unwrap() else {
            panic!("duplicate startup took over");
        };
        assert_eq!(activated, health);
        assert_eq!(activations.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn an_abandoned_or_invalid_exchange_cannot_activate_or_disable_the_listener() {
        let scope = Scope::current()
            .unwrap()
            .fixture(&format!("faults-{}", std::process::id()))
            .unwrap();
        let _listener = Reservation::bind(&scope)
            .unwrap()
            .serve(
                || panic!("invalid request activated the resident"),
                |error| panic!("listener failed: {error}"),
            )
            .unwrap();
        runtime().unwrap().block_on(async {
            for invalid in [&[][..], &[VERSION], &[99, 2], &[VERSION, 255]] {
                let mut peer = timeout(EXCHANGE, async {
                    loop {
                        match ClientOptions::new().open(scope.name()) {
                            Ok(client) => break client,
                            Err(error) if error.raw_os_error() == Some(ERROR_PIPE_BUSY as i32) => {
                                sleep(Duration::from_millis(20)).await;
                            }
                            Err(error) => panic!("could not open test peer: {error}"),
                        }
                    }
                })
                .await
                .unwrap();
                peer.write_all(invalid).await.unwrap();
                if invalid == [VERSION] {
                    // Leave the truncated peer open. The server must expire
                    // that exchange and allow the next real client through.
                    let receipt = timeout(
                        Duration::from_secs(5),
                        scope.request_when_ready(Action::Health),
                    )
                    .await
                    .expect("truncated peer monopolized the listener")
                    .unwrap();
                    assert_eq!(receipt.pid, std::process::id());
                } else if invalid.len() == 2 {
                    let mut byte = [0; 1];
                    let result = timeout(EXCHANGE, peer.read(&mut byte)).await.unwrap();
                    assert!(matches!(result, Ok(0) | Err(_)));
                }
            }
        });
        assert_eq!(
            scope.request(Action::Health).unwrap().pid,
            std::process::id()
        );
    }

    #[test]
    fn missing_server_returns_within_the_startup_budget() {
        let scope = Scope::current()
            .unwrap()
            .fixture(&format!("missing-{}", std::process::id()))
            .unwrap();
        let start = std::time::Instant::now();
        assert_eq!(
            scope.request(Action::Health).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
        assert!(start.elapsed() < Duration::from_secs(5));
    }
}

pub struct Listener {
    stop: Option<oneshot::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl Drop for Listener {
    fn drop(&mut self) {
        self.stop.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

async fn serve_client(
    server: &mut NamedPipeServer,
    session: u32,
    activate: &impl Fn() -> Option<oneshot::Receiver<()>>,
) -> io::Result<()> {
    let mut peer = 0;
    checked(unsafe { GetNamedPipeClientSessionId(server.as_raw_handle(), &mut peer) })?;
    if peer != session {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    let mut request = [0; 2];
    server.read_exact(&mut request).await?;
    let action = match request {
        [VERSION, 1] => Action::Health,
        [VERSION, 2] => {
            activate()
                .ok_or_else(|| invalid("resident unavailable"))?
                .await
                .map_err(|_| invalid("panel could not open"))?;
            Action::Activate
        }
        _ => return Err(invalid("unknown Rooom request")),
    };
    server
        .write_all(&response(action, std::process::id(), session))
        .await?;
    if server.read_u8().await? != ACK {
        return Err(invalid("invalid Rooom acknowledgement"));
    }
    Ok(())
}

pub fn activation_reply() -> (oneshot::Sender<()>, oneshot::Receiver<()>) {
    oneshot::channel()
}

fn response(action: Action, pid: u32, session: u32) -> [u8; 10] {
    let mut bytes = [VERSION, action as u8, 0, 0, 0, 0, 0, 0, 0, 0];
    bytes[2..6].copy_from_slice(&pid.to_le_bytes());
    bytes[6..].copy_from_slice(&session.to_le_bytes());
    bytes
}

fn runtime() -> io::Result<Runtime> {
    Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
}

fn invalid(reason: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, reason)
}

fn checked(success: i32) -> io::Result<()> {
    if success == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

struct LocalAllocation(*mut c_void);

impl Drop for LocalAllocation {
    fn drop(&mut self) {
        unsafe { LocalFree(self.0) };
    }
}

fn process_sid(process: HANDLE) -> io::Result<String> {
    let mut token = null_mut();
    checked(unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) })?;
    let token = OwnedHandle(token);
    let mut bytes = 0;
    unsafe { GetTokenInformation(token.0, TokenUser, null_mut(), 0, &mut bytes) };
    if bytes == 0 {
        return Err(io::Error::last_os_error());
    }
    // Keep TOKEN_USER suitably aligned and its embedded SID valid for the conversion.
    let mut data = vec![0usize; (bytes as usize).div_ceil(size_of::<usize>())];
    checked(unsafe {
        GetTokenInformation(
            token.0,
            TokenUser,
            data.as_mut_ptr().cast(),
            bytes,
            &mut bytes,
        )
    })?;
    let user = unsafe { &*data.as_ptr().cast::<TOKEN_USER>() };
    let mut text = null_mut();
    checked(unsafe { ConvertSidToStringSidW(user.User.Sid, &mut text) })?;
    let _text = LocalAllocation(text.cast());
    let count = (0..)
        .take_while(|&index| unsafe { *text.add(index) != 0 })
        .count();
    String::from_utf16(unsafe { std::slice::from_raw_parts(text, count) })
        .map_err(|_| invalid("invalid Windows SID"))
}
