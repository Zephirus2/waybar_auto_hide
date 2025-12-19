use serde::Deserialize;
use std::{
    collections::HashMap,
    io::{BufRead, BufReader},
    os::unix::net::UnixStream,
    process::{Command, Output},
    sync::{Arc, Mutex, mpsc},
    thread,
    time::Duration,
};

// The distance from the top at which the bar will activate
const PIXEL_THRESHOLD: i32 = 3;
// The distane from the top at which the bar will hide again.
const PIXEL_THRESHOLD_SECONDARY: i32 = 50;
// The delay between between mouse position updates.
const MOUSE_REFRESH_DELAY_MS: u64 = 100;

/// What: Stores waybar visibility state for tracking.
///
/// Inputs: None
///
/// Output: None
///
/// Details: This structure maintains the visibility state of waybar to avoid
/// unnecessary toggles and ensure correct behavior.
struct WaybarState {
    /// Maps state key to visibility flag
    window_positions: HashMap<String, i32>,
}

fn main() {
    let (tx, rx) = mpsc::channel::<Event>();

    let mut cursor_top: bool = false;
    let mut windows_opened: bool = check_windows();
    let mut last_visibility: bool = !windows_opened;

    // Assume waybar starts visible (default state) - important for correct toggling
    let mut initial_state = HashMap::new();
    initial_state.insert("_waybar_visible_state".to_string(), 1);

    let waybar_state = Arc::new(Mutex::new(WaybarState {
        window_positions: initial_state,
    }));

    spawn_mouse_position_updated(tx.clone());
    spawn_window_event_listener(tx.clone());

    tx.send(Event::CursorTop(false)).ok();
    tx.send(Event::WindowsOpened(windows_opened)).ok();

    // If windows are open, toggle waybar to hidden (matches assumed visible state)
    if windows_opened {
        set_waybar_visible(false, waybar_state.clone());
    }

    for event in rx {
        match event {
            Event::CursorTop(val) => {
                cursor_top = val;
            }
            Event::WindowsOpened(val) => windows_opened = val,
        }

        let current_visible = if cursor_top { true } else { !windows_opened };

        if current_visible != last_visibility {
            set_waybar_visible(current_visible, waybar_state.clone());
        }
        last_visibility = current_visible;
    }
}
/// Keeps track of the mouse position
fn spawn_mouse_position_updated(tx: mpsc::Sender<Event>) {
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

#[derive(Deserialize)]
pub struct ActiveWindows {
    windows: i32,
}

/// What: Sets waybar visibility using signal-based approach.
///
/// Inputs:
/// - `visible`: Whether waybar should be visible
/// - `state`: Shared state for tracking waybar visibility state
///
/// Output: None
///
/// Details: Uses waybar's signal system with proper state tracking to avoid
/// sync issues. Always uses SIGUSR1 for toggling visibility.
fn set_waybar_visible(visible: bool, state: Arc<Mutex<WaybarState>>) {
    let Ok(mut state_guard) = state.lock() else {
        eprintln!("Failed to acquire waybar state lock");
        return;
    };

    // SIGUSR1 toggles visibility, so we track state to avoid unnecessary toggles
    let current_state = state_guard
        .window_positions
        .contains_key("_waybar_visible_state");

    let needs_toggle = (visible && !current_state) || (!visible && current_state);

    if needs_toggle {
        // Retry mechanism ensures signal is received, especially when hiding
        let mut signal_sent = false;
        for attempt in 1..=3 {
            match Command::new("killall")
                .args(["-SIGUSR1", "waybar"])
                .output()
            {
                Ok(output) => {
                    if output.status.success() {
                        signal_sent = true;
                        break;
                    }
                }
                Err(e) => {
                    eprintln!("Failed to send SIGUSR1 to waybar: {e}");
                }
            }
            if attempt < 3 {
                thread::sleep(Duration::from_millis(50));
            }
        }

        // Extra retry for hiding with longer delay
        if !signal_sent && !visible {
            thread::sleep(Duration::from_millis(200));
            let _ = Command::new("killall")
                .args(["-SIGUSR1", "waybar"])
                .output();
        }

        thread::sleep(Duration::from_millis(100));

        if visible {
            state_guard
                .window_positions
                .insert("_waybar_visible_state".to_string(), 1);
        } else {
            state_guard.window_positions.remove("_waybar_visible_state");
        }
    }
}
