//! Linux platform runtime — /dev/tty1 output, sd_notify signals (ARCH-0002).

use std::io::Write;

use super::PlatformRuntime;

pub struct LinuxRuntime;

impl LinuxRuntime {
    pub fn new() -> Self {
        Self
    }
}

impl PlatformRuntime for LinuxRuntime {
    fn write_line(&self, text: &str) {
        match std::fs::OpenOptions::new().write(true).open("/dev/tty1") {
            Ok(mut tty) => {
                let _ = writeln!(tty, "{}", text);
            }
            Err(_) => println!("{}", text),
        }
    }

    fn notify_ready(&self) {
        let _ = sd_notify::notify(false, &[sd_notify::NotifyState::Ready]);
        tracing::debug!("sd_notify: READY=1");
    }

    fn notify_stopping(&self) {
        let _ = sd_notify::notify(false, &[sd_notify::NotifyState::Stopping]);
        tracing::debug!("sd_notify: STOPPING=1");
    }

    fn notify_watchdog(&self) {
        let _ = sd_notify::notify(false, &[sd_notify::NotifyState::Watchdog]);
    }

    fn notify_status(&self, status: &str, extend_timeout_usec: Option<u64>) {
        if let Some(usec) = extend_timeout_usec {
            let extend_msg = format!("EXTEND_TIMEOUT_USEC={}", usec);
            let _ = sd_notify::notify(
                false,
                &[
                    sd_notify::NotifyState::Status(status),
                    sd_notify::NotifyState::Custom(&extend_msg),
                ],
            );
        } else {
            let _ = sd_notify::notify(false, &[sd_notify::NotifyState::Status(status)]);
        }
    }
}
