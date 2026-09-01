use std::io::{BufRead, BufReader};
use std::{
    io::{Read, Write},
    os::unix::net::UnixStream,
    sync::mpsc,
    thread,
};

use std::{sync::mpsc::Sender, time::Duration};

use super::Event;

use crate::{
    Args, CursorPos, MOUSE_REFRESH_DELAY_MS, Monitor, PIXEL_THRESHOLD, PIXEL_THRESHOLD_SECONDARY,
    Side,
};

/// Helper to communicate with Hyprland Socket instead of spawning processes
pub fn hypr_query(cmd: &str) -> Option<String> {
    let socket_path = format!(
        "{}/hypr/{}/.socket.sock",
        std::env::var("XDG_RUNTIME_DIR").ok()?,
        std::env::var("HYPRLAND_INSTANCE_SIGNATURE").ok()?
    );
    let mut stream = UnixStream::connect(socket_path).ok()?;
    stream.write_all(cmd.as_bytes()).ok()?;
    let mut response = String::new();
    stream.read_to_string(&mut response).ok()?;
    Some(response)
}

fn get_cursor_pos() -> Option<CursorPos> {
    serde_json::from_str(&hypr_query("j/cursorpos")?).ok()
}

/// All connected monitors, each with its position in the global layout
fn get_monitors() -> Option<Vec<Monitor>> {
    serde_json::from_str(&hypr_query("j/monitors")?).ok()
}

/// Returns true of a window or more is open
pub fn check_windows() -> bool {
    let res = hypr_query("j/activeworkspace").unwrap_or_default();
    let data: serde_json::Value = serde_json::from_str(&res).unwrap_or_default();
    data["windows"].as_i64().unwrap_or(0) > 0
}

/// Watches the compositor event stream and re-checks the window count whenever something
/// might have changed it.
pub fn spawn_window_event_listener(tx: mpsc::Sender<Event>) {
    thread::spawn(move || {
        let socket_path = format!(
            "{}/hypr/{}/.socket2.sock",
            std::env::var("XDG_RUNTIME_DIR").unwrap(),
            std::env::var("HYPRLAND_INSTANCE_SIGNATURE").unwrap()
        );

        let stream = match UnixStream::connect(&socket_path) {
            Ok(s) => s,
            Err(_) => return,
        };

        let reader = BufReader::new(stream);
        for line in reader.lines().flatten() {
            if line.contains("window") || line.contains("workspace") {
                tx.send(Event::WindowsOpened(check_windows())).ok();
            }
        }
    });
}

/// Keeps track of the mouse position at a constant polling rate (100 ms by default)
pub fn spawn_mouse_position_updater(tx: Sender<Event>, args: Args) {
    thread::spawn(move || {
        let mut previous_state = false;
        loop {
            if let (Some(pos), Some(monitors)) = (get_cursor_pos(), get_monitors()) {
                // Multi-monitor fix: Find which monitor the cursor is currently on
                // Scaling fix: Mouse position in impl Monitor
                let active_monitor = monitors.iter().find(|m| m.contains(&pos));

                // Scaling fix: Scaling calculation in impl Monitor
                if let Some(m) = active_monitor {
                    let distance_from_edge = match args.side {
                        Side::Top => pos.y - m.y,
                        Side::Bottom => (m.y + m.logical_height()) - pos.y,
                        Side::Left => pos.x - m.x,
                        Side::Right => (m.x + m.logical_width()) - pos.x,
                    };

                    let threshold = if previous_state {
                        PIXEL_THRESHOLD_SECONDARY
                    } else {
                        PIXEL_THRESHOLD
                    };

                    let is_cursor_active = distance_from_edge <= threshold;

                    if is_cursor_active != previous_state {
                        tx.send(Event::CursorTop(is_cursor_active)).ok();
                    }
                    previous_state = is_cursor_active;
                }
            }
            thread::sleep(Duration::from_millis(MOUSE_REFRESH_DELAY_MS));
        }
    });
}
