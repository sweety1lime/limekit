use std::fmt::Write as _;
use std::io;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use interprocess::local_socket::{
    GenericFilePath, GenericNamespaced, Listener, ListenerOptions, Stream, prelude::*,
};

use crate::{
    Command, PROTOCOL_VERSION, Request, Response, ValidationError, read_frame, write_frame,
};

const IO_TIMEOUT: Duration = Duration::from_secs(10);

pub type LocalStream = Stream;

pub struct LocalServer {
    listener: Listener,
}

impl LocalServer {
    pub fn bind(endpoint: &str) -> Result<Self, TransportError> {
        let listener = create_listener(endpoint)?;
        Ok(Self { listener })
    }

    pub fn accept(&self) -> Result<LocalStream, TransportError> {
        let stream = self.listener.accept()?;
        configure_stream(&stream)?;
        Ok(stream)
    }
}

/// Opens a local connection, sends one request, receives one response and
/// closes the connection. A request id and protocol mismatch are rejected.
pub fn send_request(endpoint: &str, request: &Request) -> Result<Response, TransportError> {
    request.validate()?;
    let mut stream = connect(endpoint)?;
    configure_stream(&stream)?;
    write_frame(&mut stream, request)?;
    let response: Response = read_frame(&mut stream)?;
    if response.version != PROTOCOL_VERSION {
        return Err(TransportError::UnsupportedResponseVersion {
            received: response.version,
            supported: PROTOCOL_VERSION,
        });
    }
    if response.id != request.id {
        return Err(TransportError::MismatchedResponseId {
            request: request.id,
            response: response.id,
        });
    }
    Ok(response)
}

/// Per-user endpoint used by renderer and UI unless explicitly overridden.
pub fn default_endpoint() -> String {
    endpoint_with_prefix("limekit")
}

/// Endpoint a LimeWall 0.1 daemon listens on: the same protocol under the
/// product's old name. Only used to ask a still-running old daemon to quit.
pub fn legacy_endpoint() -> String {
    endpoint_with_prefix("limewall")
}

fn endpoint_with_prefix(prefix: &str) -> String {
    let user = std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_else(|_| "default".into());
    let mut tag = String::with_capacity(user.len() * 2);
    for byte in user.as_bytes().iter().take(48) {
        let _ = write!(tag, "{byte:02x}");
    }
    format!("{prefix}-v{PROTOCOL_VERSION}-{tag}.sock")
}

/// What [`stop_legacy_daemon`] found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegacyDaemon {
    /// Nothing was listening on the old endpoint.
    NotRunning,
    /// It acknowledged the shutdown and its endpoint has gone away.
    Stopped,
    /// It did not answer, or did not go away, in the time allowed.
    Unresponsive,
}

/// Asks a LimeWall 0.1 daemon, if one is running, to quit, and waits until its
/// endpoint disappears.
///
/// Upgrading testers usually still have the old daemon in the tray. It holds
/// its log open inside %APPDATA%\LimeWall, which makes the move to
/// %APPDATA%\LimeKit fail, and left alone it would fight the new daemon for the
/// layer behind the desktop icons. The old protocol is the same v1, so the
/// shutdown it already understands does the job.
///
/// Never blocks for longer than `timeout`: a hung daemon is reported as
/// [`LegacyDaemon::Unresponsive`] and the caller carries on.
pub fn stop_legacy_daemon(timeout: Duration) -> LegacyDaemon {
    stop_daemon_at(&legacy_endpoint(), timeout)
}

/// [`stop_legacy_daemon`] against an explicit endpoint, so tests never aim the
/// shutdown at a real old daemon on the developer's machine.
fn stop_daemon_at(endpoint: &str, timeout: Duration) -> LegacyDaemon {
    let deadline = Instant::now() + timeout;
    let endpoint = endpoint.to_owned();

    let (sender, receiver) = mpsc::channel();
    let request_endpoint = endpoint.clone();
    std::thread::spawn(move || {
        let request = Request::new(1, Command::Shutdown);
        let _ = sender.send(send_request(&request_endpoint, &request).is_ok());
    });
    match receiver.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
        // Nobody answered on the old endpoint: no old daemon to stop.
        Ok(false) => return LegacyDaemon::NotRunning,
        Ok(true) => {}
        Err(_) => return LegacyDaemon::Unresponsive,
    }

    // The reply comes before the daemon has torn down; wait for the endpoint
    // itself to go, which happens as the process exits.
    while Instant::now() < deadline {
        if connect(&endpoint).is_err() {
            return LegacyDaemon::Stopped;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    LegacyDaemon::Unresponsive
}

fn create_listener(endpoint: &str) -> io::Result<Listener> {
    if GenericNamespaced::is_supported() {
        let name = endpoint.to_ns_name::<GenericNamespaced>()?;
        ListenerOptions::new().name(name).create_sync()
    } else {
        let path = std::env::temp_dir().join(endpoint);
        let name = path.as_path().to_fs_name::<GenericFilePath>()?;
        ListenerOptions::new().name(name).create_sync()
    }
}

fn connect(endpoint: &str) -> io::Result<Stream> {
    if GenericNamespaced::is_supported() {
        let name = endpoint.to_ns_name::<GenericNamespaced>()?;
        Stream::connect(name)
    } else {
        let path = std::env::temp_dir().join(endpoint);
        let name = path.as_path().to_fs_name::<GenericFilePath>()?;
        Stream::connect(name)
    }
}

fn configure_stream(stream: &Stream) -> io::Result<()> {
    // Best-effort: Unix domain sockets support I/O timeouts, Windows named
    // pipes do not — there a stalled peer must be handled by the daemon's
    // connection handling instead.
    allow_unsupported(stream.set_recv_timeout(Some(IO_TIMEOUT)))?;
    allow_unsupported(stream.set_send_timeout(Some(IO_TIMEOUT)))
}

fn allow_unsupported(result: io::Result<()>) -> io::Result<()> {
    match result {
        Err(error) if error.kind() == io::ErrorKind::Unsupported => Ok(()),
        other => other,
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("local IPC transport failed: {0}")]
    Io(#[from] io::Error),
    #[error(transparent)]
    Frame(#[from] crate::FrameError),
    #[error(transparent)]
    Validation(#[from] ValidationError),
    #[error("response uses protocol version {received}; supported version is {supported}")]
    UnsupportedResponseVersion { received: u16, supported: u16 },
    #[error("response id {response} does not match request id {request}")]
    MismatchedResponseId { request: u64, response: u64 },
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use crate::{Command, ResponseBody, ResponseData};

    use super::*;

    static NEXT_ENDPOINT: AtomicU64 = AtomicU64::new(1);

    fn test_endpoint() -> String {
        let id = NEXT_ENDPOINT.fetch_add(1, Ordering::Relaxed);
        format!("limekit-ipc-test-{}-{id}.sock", std::process::id())
    }

    #[test]
    fn default_endpoint_is_namespaced_and_user_specific() {
        let endpoint = default_endpoint();
        assert!(endpoint.starts_with("limekit-v1-"));
        assert!(endpoint.ends_with(".sock"));
        assert!(!endpoint.contains('/') && !endpoint.contains('\\'));
    }

    #[test]
    fn local_transport_round_trip() {
        let endpoint = test_endpoint();
        let server = LocalServer::bind(&endpoint).expect("bind test endpoint");
        let server_thread = std::thread::spawn(move || {
            let mut stream = server.accept().expect("accept test client");
            let request: Request = read_frame(&mut stream).expect("read request");
            assert_eq!(request.command, Command::Ping);
            let response = Response::success(
                request.id,
                ResponseData::Pong {
                    daemon_version: "test".into(),
                },
            );
            write_frame(&mut stream, &response).expect("write response");
        });

        let request = Request::new(7, Command::Ping);
        let response = send_request(&endpoint, &request).expect("send request");
        assert!(matches!(
            response.body,
            ResponseBody::Success {
                result: ResponseData::Pong { daemon_version }
            } if daemon_version == "test"
        ));
        server_thread.join().expect("server thread");
    }

    #[test]
    fn legacy_endpoint_differs_from_the_current_one_only_by_name() {
        let legacy = legacy_endpoint();
        assert!(legacy.starts_with("limewall-v1-"));
        assert_eq!(
            legacy.trim_start_matches("limewall"),
            default_endpoint().trim_start_matches("limekit")
        );
    }

    #[test]
    fn stopping_with_nothing_listening_reports_not_running() {
        let outcome = stop_daemon_at(&test_endpoint(), Duration::from_secs(2));
        assert_eq!(outcome, LegacyDaemon::NotRunning);
    }

    #[test]
    fn a_daemon_that_acknowledges_and_exits_is_reported_stopped() {
        let endpoint = test_endpoint();
        let server = LocalServer::bind(&endpoint).expect("bind test endpoint");
        let server_thread = std::thread::spawn(move || {
            let mut stream = server.accept().expect("accept test client");
            let request: Request = read_frame(&mut stream).expect("read request");
            assert_eq!(request.command, Command::Shutdown);
            let response = Response::success(
                request.id,
                ResponseData::Acknowledged {
                    status: "shutting_down".into(),
                },
            );
            write_frame(&mut stream, &response).expect("write response");
            // Returning drops the listener: the endpoint goes, as when the
            // old daemon's process exits.
        });

        let outcome = stop_daemon_at(&endpoint, Duration::from_secs(5));
        server_thread.join().expect("server thread");
        assert_eq!(outcome, LegacyDaemon::Stopped);
    }
}
