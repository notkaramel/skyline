use std::sync::mpsc::Sender;
use std::time::Duration;

use chrono::{Local, Timelike};
use skyline_core::ServiceEvent;

use crate::live;
use crate::spawn_named;

pub fn spawn(tx: Sender<ServiceEvent>) {
    spawn_named("skyline-clock", move || {
        let mut last = String::new();
        loop {
            let format = live::get()
                .clock_format
                .read()
                .map(|f| f.clone())
                .unwrap_or_else(|_| "%H:%M".into());
            let now = Local::now();
            let text = now.format(&format).to_string();
            if text != last {
                last = text.clone();
                if tx.send(ServiceEvent::Clock(text)).is_err() {
                    break;
                }
            }
            // Sleep until the displayed clock text would change (not a fixed poll).
            std::thread::sleep(sleep_until_next_change(&format, now));
        }
    });
}

fn sleep_until_next_change(format: &str, now: chrono::DateTime<Local>) -> Duration {
    let nanos = u64::from(now.nanosecond());
    let sec = u64::from(now.second());
    let minute = u64::from(now.minute());

    let until_next_second = {
        let rem = 1_000_000_000u64.saturating_sub(nanos);
        Duration::from_nanos(rem.max(1))
    };

    if format_needs_seconds(format) {
        return until_next_second;
    }

    let until_next_minute = {
        let secs_left = 60u64.saturating_sub(sec).max(1);
        Duration::from_secs(secs_left).saturating_sub(Duration::from_nanos(nanos))
    };

    if format_needs_minutes(format) {
        // Cap so a hot-reloaded format is picked up within a minute.
        return until_next_minute.min(Duration::from_secs(60)).max(Duration::from_millis(20));
    }

    let until_next_hour = {
        let mins_left = 60u64.saturating_sub(minute).max(1);
        Duration::from_secs(mins_left * 60)
            .saturating_sub(Duration::from_secs(sec))
            .saturating_sub(Duration::from_nanos(nanos))
    };
    until_next_hour
        .min(Duration::from_secs(60))
        .max(Duration::from_millis(20))
}

fn format_needs_seconds(format: &str) -> bool {
    // Common chrono tokens that change every second (or finer).
    for token in [
        "%S", "%-S", "%_S", "%0S", "%s", "%f", "%N", "%T", "%X", "%r", "%.f", "%.3f", "%.6f", "%.9f",
    ] {
        if format.contains(token) {
            return true;
        }
    }
    false
}

fn format_needs_minutes(format: &str) -> bool {
    if format_needs_seconds(format) {
        return true;
    }
    for token in ["%M", "%-M", "%_M", "%0M", "%R", "%I", "%-I", "%H", "%-H", "%k", "%l", "%p", "%P"] {
        if format.contains(token) {
            return true;
        }
    }
    false
}
