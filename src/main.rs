use serde::Deserialize;
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    os::unix::net::UnixStream,
    sync::mpsc::{self, Sender},
    thread,
    time::Duration,
};

// The distance from the top at which the bar will activate
const PIXEL_THRESHOLD: i32 = 3;

// The distance from the top at which the bar will hide again.

const PIXEL_THRESHOLD_SECONDARY: i32 = 50;
const MOUSE_REFRESH_DELAY_MS: u64 = 100;

fn main() {
    let (tx, rx) = mpsc::channel::<Event>();

    let mut cursor_top: bool = false;
    let mut windows_opened: bool = check_windows();
    let mut last_visibility: bool = !windows_opened;

    spawn_mouse_position_updated(tx.clone());
    spawn_window_event_listener(tx.clone());

    tx.send(Event::CursorTop(false)).ok();
    tx.send(Event::WindowsOpened(windows_opened)).ok();

    // Cache Waybar PID to avoid repeated lookups
    let mut waybar_pid = find_waybar_pid();

    for event in rx {
        match event {
            Event::CursorTop(val) => cursor_top = val,
            Event::WindowsOpened(val) => windows_opened = val,
        }

        let current_visible = if cursor_top { true } else { !windows_opened };

        if current_visible != last_visibility {
            // Refreshes PID if it was lost or not found yet
            if waybar_pid.is_none() {
                waybar_pid = find_waybar_pid();
            }

            if let Some(pid) = waybar_pid {
                if !set_waybar_visible(pid, current_visible) {
                    // If signal fails, Waybar might have restarted
                    waybar_pid = find_waybar_pid();
                    if let Some(new_pid) = waybar_pid {
                        set_waybar_visible(new_pid, current_visible);
                    }
                }
            }
        }
        last_visibility = current_visible
    }
}

/// Keeps track of the mouse position
fn spawn_mouse_position_updated(tx: Sender<Event>) {
    thread::spawn(move || {
        let mut previous_state = false;
        loop {
            if let (Some(pos), Some(monitors)) = (get_cursor_pos(), get_monitors()) {
                // Multi-monitor fix: Find which monitor the cursor is currently on
                let active_monitor = monitors.iter().find(|m| {
                    pos.x >= m.x
                        && pos.x <= m.x + m.width
                        && pos.y >= m.y
                        && pos.y <= m.y + m.height
                });

                if let Some(m) = active_monitor {
                    let local_y = pos.y - m.y;
                    let threshold = if previous_state {
                        PIXEL_THRESHOLD_SECONDARY
                    } else {
                        PIXEL_THRESHOLD
                    };
                    let is_cursor_top = local_y <= threshold;

                    if is_cursor_top != previous_state {
                        tx.send(Event::CursorTop(is_cursor_top)).ok();
                    }
                    previous_state = is_cursor_top;
                }
            }
            thread::sleep(Duration::from_millis(MOUSE_REFRESH_DELAY_MS));
        }
    });
}

#[derive(Debug)]
enum Event {
    CursorTop(bool),
    WindowsOpened(bool),
}

/// Helper to communicate with Hyprland Socket instead of spawning processes
fn hypr_query(cmd: &str) -> Option<String> {
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

fn get_monitors() -> Option<Vec<Monitor>> {
    serde_json::from_str(&hypr_query("j/monitors")?).ok()
}

fn spawn_window_event_listener(tx: mpsc::Sender<Event>) {
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

fn check_windows() -> bool {
    let res = hypr_query("j/activeworkspace").unwrap_or_default();
    let data: serde_json::Value = serde_json::from_str(&res).unwrap_or_default();
    data["windows"].as_i64().unwrap_or(0) > 0
}

/// Uses direct syscalls to signal Waybar
fn set_waybar_visible(pid: i32, visible: bool) -> bool {
    let signal = if visible { 12 } else { 10 }; // SIGUSR2 (show), SIGUSR1 (hide)
    unsafe { libc::kill(pid, signal) == 0 }
}

fn find_waybar_pid() -> Option<i32> {
    fs::read_dir("/proc")
        .ok()?
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            if !path.is_dir() {
                return None;
            }
            let comm = fs::read_to_string(path.join("comm")).ok()?;
            if comm.trim() == "waybar" {
                path.file_name()?.to_str()?.parse::<i32>().ok()
            } else {
                None
            }
        })
        .next()
}

#[derive(Deserialize)]
struct CursorPos {
    x: i32,
    y: i32,
}

#[derive(Deserialize)]
struct Monitor {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}
