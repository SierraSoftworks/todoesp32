pub const HOSTNAME: &str = "todoesp";

pub const WIFI_SSID: &str = "";
pub const WIFI_PASSWORD: &str = "";

pub const TODOIST_API_KEY: &str = "";
pub const TODOIST_FILTER: &str = "today & !subtask & (!shared | assigned to:me)";

// Fixed UTC offset in seconds applied to all timestamps. no_std builds cannot
// evaluate POSIX TZ/DST rules, so this must be updated manually for DST changes.
// Example: Ireland is UTC+1 (3600) in summer (IST) and UTC+0 (0) in winter (GMT).
pub const UTC_OFFSET_SECONDS: i32 = 0;

// NTP server used to synchronise the system clock at boot.
pub const NTP_SERVER: &str = "pool.ntp.org";
