//! Bounded UI heartbeat diagnostics. Never kills/restarts or exposes content.
use std::sync::Mutex;
use std::time::{Duration, Instant};
static ACTION: Mutex<&'static str> = Mutex::new("idle");
pub struct Span(&'static str);
pub fn enter(action: &'static str) -> Span {
    let mut value = ACTION.lock().unwrap_or_else(|e| e.into_inner());
    let previous = *value;
    *value = action;
    Span(previous)
}
impl Drop for Span {
    fn drop(&mut self) {
        *ACTION.lock().unwrap_or_else(|e| e.into_inner()) = self.0;
    }
}
pub fn start(app: &tauri::AppHandle) {
    let handle = app.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(2));
        let (tx, rx) = std::sync::mpsc::channel();
        let started = Instant::now();
        if handle
            .run_on_main_thread(move || {
                let _ = tx.send(());
            })
            .is_err()
        {
            return;
        }
        let mut reported = false;
        loop {
            match rx.recv_timeout(Duration::from_secs(5)) {
                Ok(()) => {
                    if reported {
                        crate::dlog::log(&format!(
                            "ui: heartbeat recovered after {}ms",
                            started.elapsed().as_millis()
                        ));
                    }
                    break;
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    if !reported {
                        let action = *ACTION.lock().unwrap_or_else(|e| e.into_inner());
                        crate::dlog::log(&format!("ui: heartbeat delayed; action={action}"));
                        reported = true;
                    }
                }
                Err(_) => return,
            }
        }
    });
}
