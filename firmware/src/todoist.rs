//! HTTPS client for the Todoist filtered-tasks API.
//!
//! ## Security note
//!
//! TLS server-certificate verification is disabled ([`TlsVerify::None`]).
//! `embedded-tls` does not ship a trust store and certificate-chain validation
//! on a microcontroller is expensive; we rely on the fact that the device only
//! ever talks to `api.todoist.com` over a WPA2 network. Treat the link as
//! confidential-but-unauthenticated.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use embassy_net::Stack;
use embassy_net::dns::DnsSocket;
use embassy_net::tcp::client::{TcpClient, TcpClientState};
use embedded_io_async::Read;
use reqwless::client::{HttpClient, TlsConfig, TlsVerify};
use reqwless::request::{Method, RequestBuilder};
use todoesp_core::{Task, TaskStreamParser};

/// Number of concurrent TCP connections / per-connection buffer sizes used by
/// the reqwless client.
pub type ClientState = TcpClientState<1, 4096, 4096>;

#[derive(Debug)]
pub enum TodoistError {
    /// The HTTP request failed (connection, TLS handshake, DNS, ...).
    Request,
    /// The server returned a non-2xx status code.
    Status(u16),
    /// The response body could not be read.
    Body,
    /// The response body was not valid Todoist JSON.
    Parse,
}

pub struct TodoistClient {
    api_key: &'static str,
    filter: &'static str,
    state: &'static ClientState,
}

impl TodoistClient {
    pub fn new(api_key: &'static str, filter: &'static str, state: &'static ClientState) -> Self {
        Self {
            api_key,
            filter,
            state,
        }
    }

    /// Fetch and parse the tasks matching the configured filter.
    ///
    /// `seed` is used to seed the TLS RNG and should be different on each call.
    /// `tls_read`/`tls_write` are the TLS record buffers (each must be at least
    /// 16640 bytes). `rx_buf` holds the response status line and headers; the
    /// body itself is streamed into a heap-allocated buffer, so it does not need
    /// to be large enough for the whole response.
    pub async fn get_tasks(
        &self,
        stack: Stack<'static>,
        seed: u64,
        tls_read: &mut [u8],
        tls_write: &mut [u8],
        rx_buf: &mut [u8],
    ) -> Result<Vec<Task>, TodoistError> {
        let tcp = TcpClient::new(stack, self.state);
        let dns = DnsSocket::new(stack);
        let tls = TlsConfig::new(seed, tls_read, tls_write, TlsVerify::None);
        let mut client = HttpClient::new_with_tls(&tcp, &dns, tls);

        let url = format!(
            "https://api.todoist.com/api/v1/tasks/filter?limit=200&query={}",
            percent_encode(self.filter)
        );
        let auth = format!("Bearer {}", self.api_key);
        let headers = [("Authorization", auth.as_str())];

        log::info!("Requesting tasks from Todoist API");
        let mut request = client
            .request(Method::GET, &url)
            .await
            .map_err(|_| TodoistError::Request)?
            .headers(&headers);

        let response = request
            .send(rx_buf)
            .await
            .map_err(|_| TodoistError::Request)?;

        let status = response.status;
        let content_length = response.content_length;
        log::info!(
            "Todoist API responded: HTTP {} (content-length: {:?})",
            status.0,
            content_length
        );
        if !status.is_successful() {
            log::error!("Unexpected status code from Todoist API: HTTP {}", status.0);
            return Err(TodoistError::Status(status.0));
        }

        // Parse the response incrementally as it streams off the network. The
        // body is frequently larger than the free heap, so buffering it whole
        // (or letting reqwless read it into a fixed buffer) risks an allocation
        // failure; the streaming parser only ever holds a single task object at
        // a time.
        let mut parser = TaskStreamParser::new();
        let mut tasks: Vec<Task> = Vec::new();
        let mut reader = response.body().reader();
        let mut chunk = [0u8; 512];
        let mut total = 0usize;
        loop {
            let read = reader
                .read(&mut chunk)
                .await
                .map_err(|_| TodoistError::Body)?;
            if read == 0 {
                break;
            }
            total += read;
            parser.feed(&chunk[..read], &mut tasks).map_err(|e| {
                log::error!("Failed to parse Todoist task JSON: {e:?}");
                TodoistError::Parse
            })?;
        }

        tasks.sort();
        log::info!(
            "Parsed {} tasks from {} bytes of Todoist response",
            tasks.len(),
            total
        );

        Ok(tasks)
    }
}

/// Percent-encodes a string for safe use as a URL query parameter value,
/// escaping everything except the RFC 3986 unreserved characters.
fn percent_encode(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}
