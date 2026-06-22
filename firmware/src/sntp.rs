//! Minimal SNTP (NTP) client over UDP for one-shot clock synchronisation.
//!
//! `chrono::Local` is unavailable in `no_std`, so the device learns the current
//! wall-clock time once at boot via NTP and thereafter advances it using the
//! monotonic embassy timer plus the configured fixed UTC offset.

use embassy_net::dns::DnsQueryType;
use embassy_net::udp::{PacketMetadata, UdpSocket};
use embassy_net::{IpEndpoint, Stack};
use embassy_time::{Duration, with_timeout};
use static_cell::StaticCell;

/// Seconds between the NTP epoch (1900-01-01) and the Unix epoch (1970-01-01).
const NTP_TO_UNIX: i64 = 2_208_988_800;
/// Local UDP port the client binds to.
const LOCAL_PORT: u16 = 50123;
/// Standard NTP server port.
const NTP_PORT: u16 = 123;

#[derive(Debug)]
pub enum SntpError {
    /// DNS resolution of the NTP server failed.
    Dns,
    /// DNS returned no addresses.
    NoAddress,
    /// Binding the local UDP socket failed.
    Bind,
    /// Sending the NTP request failed.
    Send,
    /// No response arrived within the timeout.
    Timeout,
    /// The response was too short to contain a transmit timestamp.
    ShortResponse,
}

/// Query `server` over NTP and return the current time as a Unix timestamp
/// (seconds since 1970-01-01 UTC).
pub async fn sync_unix_time(stack: Stack<'static>, server: &str) -> Result<i64, SntpError> {
    let addresses = stack
        .dns_query(server, DnsQueryType::A)
        .await
        .map_err(|_| SntpError::Dns)?;
    let address = addresses.into_iter().next().ok_or(SntpError::NoAddress)?;

    static RX_META: StaticCell<[PacketMetadata; 4]> = StaticCell::new();
    static RX_BUF: StaticCell<[u8; 256]> = StaticCell::new();
    static TX_META: StaticCell<[PacketMetadata; 4]> = StaticCell::new();
    static TX_BUF: StaticCell<[u8; 256]> = StaticCell::new();

    let mut socket = UdpSocket::new(
        stack,
        RX_META.init([PacketMetadata::EMPTY; 4]),
        RX_BUF.init([0; 256]),
        TX_META.init([PacketMetadata::EMPTY; 4]),
        TX_BUF.init([0; 256]),
    );
    socket.bind(LOCAL_PORT).map_err(|_| SntpError::Bind)?;

    // NTPv3 client request: LI = 0, VN = 3, Mode = 3.
    let mut request = [0u8; 48];
    request[0] = 0x1B;

    socket
        .send_to(&request, IpEndpoint::new(address, NTP_PORT))
        .await
        .map_err(|_| SntpError::Send)?;

    // The transmit timestamp (server's idea of "now") is the 4-byte big-endian
    // seconds field at offset 40 in the 48-byte NTP packet.
    let seconds = with_timeout(
        Duration::from_secs(10),
        socket.recv_from_with(|buf, _meta| {
            if buf.len() < 44 {
                None
            } else {
                Some(u32::from_be_bytes([buf[40], buf[41], buf[42], buf[43]]))
            }
        }),
    )
    .await
    .map_err(|_| SntpError::Timeout)?;

    let ntp_seconds = seconds.ok_or(SntpError::ShortResponse)?;
    Ok(ntp_seconds as i64 - NTP_TO_UNIX)
}
