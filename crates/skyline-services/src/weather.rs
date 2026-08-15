use std::process::Command;
use std::sync::atomic::Ordering;
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::Timelike;
use serde_json::Value;
use skyline_core::{ServiceEvent, WeatherSnapshot};
use tracing::warn;

use crate::live;
use crate::spawn_named;
use crate::ServiceTx;

/// Minimum refresh so a typo does not hammer wttr.in.
const MIN_INTERVAL_MS: u64 = 60_000;

pub fn spawn(tx: ServiceTx) {
    spawn_named("skyline-weather", move || {
        let mut last: Option<WeatherSnapshot> = None;
        loop {
            let location = live::get()
                .weather_location
                .read()
                .map(|s| s.clone())
                .unwrap_or_default();

            match fetch_weather(&location) {
                Ok(snap) => {
                    if last.as_ref() != Some(&snap) {
                        last = Some(snap.clone());
                        if tx.send(ServiceEvent::Weather(snap)).is_err() {
                            break;
                        }
                    }
                }
                Err(err) => warn!("weather: {err}"),
            }

            if sleep_until_next(&location) {
                // Location changed during the wait — refetch immediately.
                continue;
            }
        }
    });
}

fn sleep_until_next(last_location: &str) -> bool {
    let total = live::get()
        .weather_interval_ms
        .load(Ordering::Relaxed)
        .max(MIN_INTERVAL_MS);
    let mut slept = 0u64;
    while slept < total {
        let loc = live::get()
            .weather_location
            .read()
            .map(|s| s.clone())
            .unwrap_or_default();
        if loc != last_location {
            return true;
        }
        let step = (total - slept).min(200);
        std::thread::sleep(Duration::from_millis(step));
        slept += step;
    }
    false
}

fn fetch_weather(location: &str) -> Result<WeatherSnapshot> {
    let url = wttr_url(location);
    // wttr.in's IPv6 endpoint often fails TLS (SNI / unrecognized name).
    // Prefer IPv4, then retry without `-4` for IPv4-less networks.
    let stdout = curl_get(&url, true).or_else(|ipv4_err| match curl_get(&url, false) {
        Ok(body) => Ok(body),
        Err(_) => Err(ipv4_err),
    })?;
    parse_wttr_json(&stdout)
}

fn curl_get(url: &str, ipv4: bool) -> Result<String> {
    let mut cmd = Command::new("curl");
    cmd.args(["-fsS", "-A", "skyline", "--max-time", "20"]);
    if ipv4 {
        cmd.arg("-4");
    }
    cmd.arg(url);
    let output = cmd.output().context("running curl for wttr.in")?;
    if !output.status.success() {
        anyhow::bail!(
            "curl {} failed: {}",
            url,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn wttr_url(location: &str) -> String {
    let loc = location.trim();
    if loc.is_empty() {
        "https://wttr.in/?format=j1".into()
    } else {
        format!("https://wttr.in/{}?format=j1", encode_location(loc))
    }
}

fn encode_location(loc: &str) -> String {
    let mut out = String::new();
    for b in loc.trim().bytes() {
        match b {
            b' ' => out.push('+'),
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b',' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn parse_wttr_json(raw: &str) -> Result<WeatherSnapshot> {
    let root: Value = serde_json::from_str(raw).context("parsing wttr.in JSON")?;
    let current = first_obj(&root, "current_condition").context("missing current_condition")?;
    let area = first_obj(&root, "nearest_area");
    let today = first_obj(&root, "weather");

    let condition = nested_value(current, "weatherDesc").unwrap_or_default();
    let code = json_u32(current.get("weatherCode"));
    let emoji = emoji_for_code(code, &condition, today);

    let location = area
        .and_then(format_area)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Local".into());

    Ok(WeatherSnapshot {
        emoji,
        condition,
        location,
        temp_c: json_f32(current.get("temp_C")).unwrap_or(0.0),
        temp_f: json_f32(current.get("temp_F")).unwrap_or(0.0),
        feels_c: json_f32(current.get("FeelsLikeC")).unwrap_or(0.0),
        feels_f: json_f32(current.get("FeelsLikeF")).unwrap_or(0.0),
        humidity: json_u8(current.get("humidity")),
        wind_kmph: json_f32(current.get("windspeedKmph")),
        wind_mph: json_f32(current.get("windspeedMiles")),
        wind_dir: json_string(current.get("winddir16Point")),
        precip_mm: json_f32(current.get("precipMM")),
        pressure_hpa: json_f32(current.get("pressure")),
        uv_index: json_u8(current.get("uvIndex")),
        high_c: today.and_then(|t| json_f32(t.get("maxtempC"))),
        high_f: today.and_then(|t| json_f32(t.get("maxtempF"))),
        low_c: today.and_then(|t| json_f32(t.get("mintempC"))),
        low_f: today.and_then(|t| json_f32(t.get("mintempF"))),
    })
}

fn first_obj<'a>(v: &'a Value, key: &str) -> Option<&'a Value> {
    v.get(key)?.as_array()?.first()
}

fn nested_value(v: &Value, key: &str) -> Option<String> {
    let item = v.get(key)?.as_array()?.first()?;
    json_string(item.get("value"))
}

fn format_area(area: &Value) -> Option<String> {
    let name = nested_value(area, "areaName")?;
    let region = nested_value(area, "region").unwrap_or_default();
    if region.is_empty() || region.eq_ignore_ascii_case(&name) {
        Some(name)
    } else {
        Some(format!("{name}, {region}"))
    }
}

fn json_string(v: Option<&Value>) -> Option<String> {
    match v? {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn json_f32(v: Option<&Value>) -> Option<f32> {
    match v? {
        Value::Number(n) => n.as_f64().map(|x| x as f32),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

fn json_u8(v: Option<&Value>) -> Option<u8> {
    json_f32(v).map(|n| n.round().clamp(0.0, 255.0) as u8)
}

fn json_u32(v: Option<&Value>) -> Option<u32> {
    json_f32(v).map(|n| n.round().max(0.0) as u32)
}

fn emoji_for_code(code: Option<u32>, desc: &str, today: Option<&Value>) -> String {
    let night = is_night(today);
    if let Some(code) = code {
        if let Some(e) = emoji_from_wwo_code(code, night) {
            return e.into();
        }
    }
    emoji_from_desc(desc, night).into()
}

fn is_night(today: Option<&Value>) -> bool {
    let Some(astro) = today.and_then(|t| first_obj(t, "astronomy")) else {
        return false;
    };
    let Some(sunrise) = json_string(astro.get("sunrise")).and_then(|s| parse_ampm_minutes(&s)) else {
        return false;
    };
    let Some(sunset) = json_string(astro.get("sunset")).and_then(|s| parse_ampm_minutes(&s)) else {
        return false;
    };
    let now = chrono::Local::now();
    let mins = now.hour() * 60 + now.minute();
    mins < sunrise || mins >= sunset
}

fn parse_ampm_minutes(s: &str) -> Option<u32> {
    let s = s.trim();
    let (time, ampm) = s.rsplit_once(' ')?;
    let (h, m) = time.split_once(':')?;
    let mut hour: u32 = h.parse().ok()?;
    let minute: u32 = m.parse().ok()?;
    let pm = ampm.eq_ignore_ascii_case("pm");
    let am = ampm.eq_ignore_ascii_case("am");
    if !pm && !am {
        return None;
    }
    if hour == 12 {
        hour = 0;
    }
    if pm {
        hour += 12;
    }
    Some(hour * 60 + minute)
}

fn emoji_from_wwo_code(code: u32, night: bool) -> Option<&'static str> {
    Some(match code {
        113 => {
            if night {
                "🌙"
            } else {
                "☀️"
            }
        }
        116 => "⛅",
        119 | 122 => "☁️",
        143 | 248 | 260 => "🌫️",
        176 | 263 | 266 | 293 | 353 => "🌦️",
        179 | 182 | 317 | 320 | 323 | 326 | 350 | 362 | 365 | 368 | 374 | 377 => "🌨️",
        185 | 281 | 284 | 296 | 299 | 302 | 305 | 308 | 311 | 314 | 356 | 359 => "🌧️",
        200 | 386 | 389 | 392 | 395 => "⛈️",
        227 | 230 | 329 | 332 | 335 | 338 | 371 => "❄️",
        _ => return None,
    })
}

fn emoji_from_desc(desc: &str, night: bool) -> &'static str {
    let d = desc.to_ascii_lowercase();
    if d.contains("thunder") {
        "⛈️"
    } else if d.contains("blizzard") || d.contains("snow") {
        if d.contains("rain") || d.contains("sleet") {
            "🌨️"
        } else {
            "❄️"
        }
    } else if d.contains("sleet") || d.contains("ice pellet") {
        "🌨️"
    } else if d.contains("drizzle") || (d.contains("rain") && d.contains("patchy")) {
        "🌦️"
    } else if d.contains("rain") || d.contains("shower") {
        "🌧️"
    } else if d.contains("fog") || d.contains("mist") || d.contains("haze") {
        "🌫️"
    } else if d.contains("overcast") || d.contains("cloud") {
        if d.contains("partly") {
            "⛅"
        } else {
            "☁️"
        }
    } else if d.contains("clear") || d.contains("sunny") {
        if night {
            "🌙"
        } else {
            "☀️"
        }
    } else {
        "🌡️"
    }
}
