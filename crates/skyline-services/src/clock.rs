use std::sync::mpsc::Sender;
use std::time::Duration;

use chrono::Local;
use skyline_core::ServiceEvent;

use crate::live;
use crate::spawn_named;

pub fn spawn(tx: Sender<ServiceEvent>) {
    spawn_named("skyline-clock", move || loop {
        let format = live::get()
            .clock_format
            .read()
            .map(|f| f.clone())
            .unwrap_or_else(|_| "%H:%M".into());
        let now = Local::now().format(&format).to_string();
        if tx.send(ServiceEvent::Clock(now)).is_err() {
            break;
        }
        let _ = tx.send(ServiceEvent::Tick);
        std::thread::sleep(Duration::from_millis(500));
    });
}
