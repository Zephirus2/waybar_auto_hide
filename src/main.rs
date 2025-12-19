use serde::Deserialize;
use std::{
    io::{BufRead, BufReader},
    os::unix::net::UnixStream,
    process::{Command, Output},
    sync::mpsc::{self, Sender},
    thread,
    time::Duration,
};

// The distance from the top at which the bar will activate
const PIXEL_THRESHOLD: i32 = 3;
// The distane from the top at which the bar will hide again.
const PIXEL_THRESHOLD_SECONDARY: i32 = 50;
// The delay between between mouse position updates.
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

    // Main loop
    for event in rx {
        match event {
            Event::CursorTop(val) => {
                cursor_top = val;
            }
            Event::WindowsOpened(val) => windows_opened = val,
        }

        let current_visible = if cursor_top { true } else { !windows_opened };

        if current_visible != last_visibility {
            toggle_waybar_visible();
        }
        last_visibility = current_visible
    }
}
/// Keeps track of the mouse position
fn spawn_mouse_position_updated(tx: Sender<Event>) {
    thread::spawn(move || {
        let mut previous_state = false;
        loop {
            let cursor_pos: Option<CursorPos> = get_cursor_pos();
            if let Some(pos) = cursor_pos {
                let treshold = if previous_state {
                    PIXEL_THRESHOLD_SECONDARY
                } else {
                    PIXEL_THRESHOLD
                };
                let is_cursor_top: bool = pos.y <= treshold;
                if is_cursor_top != previous_state {
                    tx.send(Event::CursorTop(is_cursor_top)).ok();
                }
                previous_state = is_cursor_top;
                thread::sleep(Duration::from_millis(MOUSE_REFRESH_DELAY_MS));
            }
        }
    });
}
#[derive(Debug)]
enum Event {
    CursorTop(bool),
    WindowsOpened(bool),
}

fn get_cursor_pos() -> Option<CursorPos> {
    let output: Output = Command::new("hyprctl")
        .args(["-j", "cursorpos"])
        .output()
        .ok()?;
    serde_json::from_slice(&output.stdout).ok()
}

#[derive(Deserialize)]
struct CursorPos {
    y: i32,
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
            Err(e) => {
                eprintln!("Failed to connect to socket: {e}");
                return;
            }
        };

        let reader: BufReader<UnixStream> = BufReader::new(stream);

        for line in reader.lines() {
            if let Ok(line) = line
                && (line.contains("openwindow")
                    || line.contains("closewindow")
                    || line.contains("workspace"))
            {
                tx.send(Event::WindowsOpened(check_windows())).ok();
            }
        }
    });
}

/// Checks the amount of windows opened, if there is none, return false.
fn check_windows() -> bool {
    let opened_windows: Option<ActiveWindows> = Command::new("hyprctl")
        .args(["activeworkspace", "-j"])
        .output()
        .ok()
        .and_then(|output| serde_json::from_slice(&output.stdout).ok());

    if let Some(active) = opened_windows {
        active.windows > 0
    } else {
        false
    }
}

fn toggle_waybar_visible() {
    Command::new("killall")
        .args(["-SIGUSR1", "waybar"])
        .output()
        .ok();
}

#[derive(Deserialize)]
pub struct ActiveWindows {
    windows: i32,
}
