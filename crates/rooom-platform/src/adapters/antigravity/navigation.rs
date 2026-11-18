//! Navigation to a fixed cascade route in the verified local Desktop.

use super::{owns_connection, owns_listener, parse_response, read_announcement};
use crate::{adapters::windows::DesktopWindow, pipe::Scope};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{Ipv4Addr, SocketAddr, TcpStream},
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};
use tungstenite::{Message, client::client_with_config, protocol::WebSocketConfig};

const UNAVAILABLE: &str = "The local Antigravity Desktop page is unavailable.";
const FAILED: &str = "Antigravity did not open the verified conversation.";
const IO_TIMEOUT: Duration = Duration::from_millis(800);
const NAVIGATION_TIMEOUT: Duration = Duration::from_secs(4);
const FRAME_LIMIT: usize = 256 * 1024;
const HEADER_LIMIT: u64 = 8192;

pub(super) fn cascade_id(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
}

pub(super) struct Route<'a> {
    scope: &'a Scope,
    window: DesktopWindow,
    port: u16,
    origin: String,
}

impl<'a> Route<'a> {
    pub(super) fn new(scope: &'a Scope, executable: &Path, https_port: u16) -> Option<Self> {
        let profile = PathBuf::from(std::env::var_os("APPDATA")?).join("Antigravity");
        if !profile.is_absolute() {
            return None;
        }
        let bytes = read_announcement(&profile.join("DevToolsActivePort")).ok()?;
        let port: u16 = std::str::from_utf8(&bytes)
            .ok()?
            .lines()
            .next()?
            .parse()
            .ok()?;
        if port == 0 {
            return None;
        }
        let window = DesktopWindow::find(scope, executable, |pid| owns_listener(pid, port))?;
        Some(Self {
            scope,
            window,
            port,
            origin: format!("https://127.0.0.1:{https_port}"),
        })
    }

    pub(super) fn available(&self) -> bool {
        self.page(Instant::now() + IO_TIMEOUT)
            .is_ok_and(|page| page.is_some())
    }

    pub(super) fn open(self, cascade: &str) -> Result<DesktopWindow, &'static str> {
        if !cascade_id(cascade) {
            return Err(FAILED);
        }
        let deadline = Instant::now() + NAVIGATION_TIMEOUT;
        let page = self.page(deadline)?.ok_or(UNAVAILABLE)?;
        let destination = format!("{}/c/{cascade}", self.origin);
        if page.route() == destination {
            return Ok(self.window);
        }

        self.navigate(&page, &destination, deadline)?;
        while Instant::now() < deadline {
            if self
                .page(deadline)?
                .is_some_and(|current| current.id == page.id && current.route() == destination)
            {
                return Ok(self.window);
            }
            thread::sleep(Duration::from_millis(50));
        }
        Err(FAILED)
    }

    fn connect(&self, deadline: Instant) -> Result<TcpStream, &'static str> {
        if !self.window.valid(self.scope) {
            return Err(UNAVAILABLE);
        }
        let address = SocketAddr::from((Ipv4Addr::LOCALHOST, self.port));
        let stream =
            TcpStream::connect_timeout(&address, remaining(deadline)?).map_err(|_| UNAVAILABLE)?;
        let peer = stream.local_addr().map_err(|_| UNAVAILABLE)?.port();
        if !owns_connection(self.window.pid, self.port, peer) {
            return Err(UNAVAILABLE);
        }
        let timeout = Some(remaining(deadline)?);
        stream.set_read_timeout(timeout).map_err(|_| UNAVAILABLE)?;
        stream.set_write_timeout(timeout).map_err(|_| UNAVAILABLE)?;
        Ok(stream)
    }

    fn page(&self, deadline: Instant) -> Result<Option<Page>, &'static str> {
        let mut socket = self.connect(deadline)?;
        write!(
            socket,
            "GET /json/list HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
            self.port
        )
        .map_err(|_| UNAVAILABLE)?;
        let body = read_inventory(BufReader::new(socket)).ok_or(UNAVAILABLE)?;
        let pages = serde_json::from_slice(&body).map_err(|_| UNAVAILABLE)?;
        local_page(pages, &self.origin, self.port)
    }

    fn navigate(&self, page: &Page, url: &str, deadline: Instant) -> Result<(), &'static str> {
        let stream = self.connect(deadline)?;
        let config = WebSocketConfig::default()
            .max_message_size(Some(FRAME_LIMIT))
            .max_frame_size(Some(FRAME_LIMIT));
        let address = page.web_socket_debugger_url.as_deref().ok_or(UNAVAILABLE)?;
        let (mut socket, _) =
            client_with_config(address, stream, Some(config)).map_err(|_| UNAVAILABLE)?;
        let request = json!({"id": 1, "method": "Page.navigate", "params": {"url": url}});
        socket
            .send(Message::Text(request.to_string().into()))
            .map_err(|_| FAILED)?;
        loop {
            socket
                .get_mut()
                .set_read_timeout(Some(remaining(deadline)?))
                .map_err(|_| FAILED)?;
            let message = socket.read().map_err(|_| FAILED)?;
            if message.is_close() {
                return Err(FAILED);
            }
            let Message::Text(message) = message else {
                continue;
            };
            let reply: Value = serde_json::from_str(&message).map_err(|_| FAILED)?;
            if reply["id"] == 1 {
                let result = &reply["result"];
                return (reply.get("error").is_none()
                    && result.get("errorText").is_none()
                    && result["isDownload"] != true
                    && result["frameId"].as_str().is_some_and(|id| !id.is_empty()))
                .then_some(())
                .ok_or(FAILED);
            }
        }
    }
}

fn read_inventory(mut input: impl BufRead) -> Option<Vec<u8>> {
    let mut reply = Vec::new();
    let mut headers = input.by_ref().take(HEADER_LIMIT);
    loop {
        let offset = reply.len();
        if headers.read_until(b'\n', &mut reply).ok()? == 0 {
            return None;
        }
        if &reply[offset..] == b"\r\n" {
            break;
        }
    }
    let mut lengths = std::str::from_utf8(&reply)
        .ok()?
        .split("\r\n")
        .filter_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("Content-Length")
                .then_some(value.trim())
        });
    let size = lengths.next()?.parse::<usize>().ok()?;
    if lengths.next().is_some() || size > FRAME_LIMIT - reply.len() {
        return None;
    }
    // Chromium keeps the local HTTP connection open after returning the list.
    let start = reply.len();
    reply.resize(start + size, 0);
    input.read_exact(&mut reply[start..]).ok()?;
    parse_response(&reply)
}

fn remaining(deadline: Instant) -> Result<Duration, &'static str> {
    let left = deadline.saturating_duration_since(Instant::now());
    if left.is_zero() {
        Err(FAILED)
    } else {
        Ok(left.min(IO_TIMEOUT))
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Page {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    url: String,
    web_socket_debugger_url: Option<String>,
}

impl Page {
    fn route(&self) -> &str {
        self.url.split(['?', '#']).next().unwrap_or("")
    }
}

fn local_page(pages: Vec<Page>, origin: &str, port: u16) -> Result<Option<Page>, &'static str> {
    let mut local = pages.into_iter().filter(|page| {
        page.kind == "page"
            && page
                .url
                .strip_prefix(origin)
                .is_some_and(|rest| rest.starts_with('/'))
    });
    let Some(page) = local.next() else {
        return Ok(None);
    };
    if local.next().is_some()
        || page.id.len() != 32
        || !page.id.bytes().all(|byte| byte.is_ascii_hexdigit())
        || page.web_socket_debugger_url.as_deref()
            != Some(&format!("ws://127.0.0.1:{port}/devtools/page/{}", page.id))
    {
        return Err(UNAVAILABLE);
    }
    Ok(Some(page))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORIGIN: &str = "https://127.0.0.1:48001";
    const PORT: u16 = 48002;
    const PAGE: &str = "9999999999999999BBBBBBBBBBBBBBBB";

    fn page(url: &str, socket: String) -> Page {
        Page {
            id: PAGE.into(),
            kind: "page".into(),
            url: url.into(),
            web_socket_debugger_url: Some(socket),
        }
    }

    #[test]
    fn navigation_accepts_only_an_unambiguous_service_page_and_its_owned_socket_route() {
        let socket = format!("ws://127.0.0.1:{PORT}/devtools/page/{PAGE}");
        let selected = format!("{ORIGIN}/");
        assert!(
            local_page(vec![page(&selected, socket.clone())], ORIGIN, PORT)
                .unwrap()
                .is_some()
        );
        for url in [
            "https://127.0.0.1:480010/",
            "https://localhost:48001/",
            "about:blank",
        ] {
            assert!(
                local_page(vec![page(url, socket.clone())], ORIGIN, PORT)
                    .unwrap()
                    .is_none()
            );
        }
        for target in [
            format!("{socket}?ignored=true"),
            socket.replace("48002", "48003"),
            socket.replace("127.0.0.1", "example.com"),
        ] {
            assert!(local_page(vec![page(&selected, target)], ORIGIN, PORT).is_err());
        }
        let ambiguous = vec![page(&selected, socket.clone()), page(&selected, socket)];
        assert!(local_page(ambiguous, ORIGIN, PORT).is_err());
    }

    #[test]
    fn cascade_id_cannot_supply_a_route_or_query() {
        assert!(cascade_id("005ca7e5-5dcf-41a3-9db3-617b58cb9f48"));
        for invalid in [
            "session-1",
            "../outside",
            "005ca7e5-5dcf-41a3-9db3-617b58cb9f48?",
        ] {
            assert!(!cascade_id(invalid));
        }
    }

    #[test]
    fn reads_one_content_length_frame_without_waiting_for_socket_close() {
        let response = b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n[]next";
        let mut input = &response[..];
        assert_eq!(read_inventory(&mut input), Some(b"[]".to_vec()));
        assert_eq!(input, b"next");
        assert!(read_inventory(&b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n["[..]).is_none());
    }
}
