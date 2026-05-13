use chrono::{NaiveDate, TimeZone, FixedOffset};

#[inline(always)]
fn bytes_to_int(bytes: &[u8]) -> u32 {
    let mut res = 0u32;
    for &b in bytes {
        res = res * 10 + (b - b'0') as u32;
    }
    res
}

/// Parses YYYY-MM-DD and HH:MM:SS as Taiwan (UTC+8) time and returns a UTC Unix Timestamp.
pub fn date_time_to_timestamp(date_bytes: &[u8], time_bytes: &[u8]) -> u64 {
    let year = bytes_to_int(&date_bytes[0..4]) as i32;
    let month = bytes_to_int(&date_bytes[5..7]);
    let day = bytes_to_int(&date_bytes[8..10]);

    let hour = bytes_to_int(&time_bytes[0..2]);
    let min = bytes_to_int(&time_bytes[3..5]);
    let sec = bytes_to_int(&time_bytes[6..8]);

    let naive_date = NaiveDate::from_ymd_opt(year, month, day).unwrap();
    let naive_datetime = naive_date.and_hms_opt(hour, min, sec).unwrap();

    // Taiwan is UTC+8
    let offset = FixedOffset::east_opt(8 * 3600).unwrap();
    offset.from_local_datetime(&naive_datetime).unwrap().timestamp() as u64
}

pub fn timestamp_to_string(ts: u64) -> String {
    // Convert UTC Unix timestamp to Taiwan (UTC+8) display string
    let offset = FixedOffset::east_opt(8 * 3600).unwrap();
    let dt = offset.timestamp_opt(ts as i64, 0).unwrap();
    dt.format("%Y-%m-%d %H:%M:%S").to_string()
}

pub fn timestamp_to_date_string(ts: u64) -> String {
    let offset = FixedOffset::east_opt(8 * 3600).unwrap();
    let dt = offset.timestamp_opt(ts as i64, 0).unwrap();
    dt.format("%Y-%m-%d").to_string()
}

pub fn timestamp_to_time_string(ts: u64) -> String {
    let offset = FixedOffset::east_opt(8 * 3600).unwrap();
    let dt = offset.timestamp_opt(ts as i64, 0).unwrap();
    dt.format("%H:%M:%S").to_string()
}

/// Converts a UTC timestamp into a Logical Day Index (days since Unix epoch).
/// Applies 3 AM logical boundary shift based on Taiwan local time.
#[inline(always)]
pub fn logical_day_index(ts: u64) -> i64 {
    // Taiwan is UTC+8 (+28800). 
    // Logical day starts at 3 AM (+10800).
    // To make 3 AM the "start" of a mathematical 86400 block, 
    // we shift the local time BACK by 3 hours.
    // 28800 (UTC+8) - 10800 (3 AM) = 18000
    let adjusted = ts as i64 + 18000;
    adjusted.div_euclid(86400)
}

/// Converts a Logical Day Index into a YYYYMMDD u32.
pub fn day_index_to_yyyymmdd(day: i64) -> u32 {
    let (y, m, d) = civil_from_days(day);
    (y as u32) * 10000 + (m as u32) * 100 + (d as u32)
}

/// Formats a Logical Day Index into a "YYYY-MM-DD" string.
pub fn format_day_index(day: i64) -> String {
    let (y, m, d) = civil_from_days(day);
    format!("{:04}-{:02}-{:02}", y, m, d)
}

/// Converts a fixed-size byte array into a string slice, trimming null bytes.
pub fn format_on_off_code(code: &[u8; 8]) -> &str {
    let len = code.iter().position(|&b| b == 0).unwrap_or(8);
    std::str::from_utf8(&code[..len]).unwrap_or("")
}

/// High-performance conversion from epoch days to Year, Month, Day.
/// Algorithm by Howard Hinnant (Public Domain).
fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = (yoe as i32) + era as i32 * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (y + (if m <= 2 { 1 } else { 0 }), m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fast_bytes_to_timestamp() {
        let date_b = b"2026-05-05";
        let time_b = b"13:33:34";

        let ts = date_time_to_timestamp(date_b, time_b);
        
        let recovered = timestamp_to_string(ts);
        assert_eq!(recovered, "2026-05-05 13:33:34");
    }

    #[test]
    fn test_day_arithmetic() {
        // 2025-02-11 02:59:59 (Before 3 AM -> Logical Day Feb 10)
        let ts_early = date_time_to_timestamp(b"2025-02-11", b"02:59:59");
        let idx_early = logical_day_index(ts_early);
        assert_eq!(day_index_to_yyyymmdd(idx_early), 20250210);

        // 2025-02-11 03:00:01 (After 3 AM -> Logical Day Feb 11)
        let ts_late = date_time_to_timestamp(b"2025-02-11", b"03:00:01");
        let idx_late = logical_day_index(ts_late);
        assert_eq!(day_index_to_yyyymmdd(idx_late), 20250211);
    }
}
