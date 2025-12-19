use serde::Deserialize;
use std::{
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

/// Maximum number of retry attempts when sending visibility signal to waybar.
/// This provides resilience against transient failures in signal delivery.
const WAYBAR_SIGNAL_RETRY_COUNT: u32 = 3;

/// Delay between retry attempts when sending visibility signal to waybar.
/// This gives waybar time to process the previous signal before retrying.
const WAYBAR_SIGNAL_RETRY_DELAY_MS: u64 = 50;

/// Delay before extra retry when initial retries fail.
/// This longer delay gives waybar additional time to process the signal
/// before attempting the extra retry.
const WAYBAR_EXTRA_RETRY_DELAY_MS: u64 = 200;

/// Delay after successfully sending visibility signal to waybar.
/// This allows waybar time to process the signal before updating internal state.
const WAYBAR_SIGNAL_POST_DELAY_MS: u64 = 100;

/// Stores waybar visibility state for tracking.
///
/// This structure maintains the visibility state of waybar to avoid
/// unnecessary toggles and ensure correct behavior.
///
/// # State Synchronization
///
/// The initial state assumes waybar is visible. If the application restarts
/// while waybar is actually hidden, there will be a temporary state mismatch.
/// This will self-correct on the next visibility change, as the toggle logic
/// will detect the mismatch and correct it. Waybar does not provide a way to
/// query its current visibility state, so we cannot synchronize the initial state.
struct WaybarState {
    /// Tracks whether waybar is currently visible
    is_visible: bool,
}

fn main() {
    let (tx, rx) = mpsc::channel::<Event>();

    let mut cursor_top: bool = false;
    let mut windows_opened: bool = check_windows();
    let mut last_visibility: bool = !windows_opened;

    // Check if waybar is running at startup
    if !is_waybar_running() {
        eprintln!(
            "Warning: waybar process not found at startup. \
             Ensure waybar is running for this utility to work correctly."
        );
    }

    // Note: We assume waybar starts visible (default state). If the application
    // restarts while waybar is actually hidden, there will be a temporary state
    // mismatch. This will self-correct on the next visibility change, as the
    // toggle logic will detect the mismatch and correct it. Waybar does not
    // provide a way to query its current visibility state, so we cannot
    // synchronize the initial state.
    let waybar_state = Arc::new(Mutex::new(WaybarState { is_visible: true }));

    spawn_mouse_position_updated(tx.clone());
    spawn_window_event_listener(tx.clone());

    tx.send(Event::CursorTop(false)).ok();
    tx.send(Event::WindowsOpened(windows_opened)).ok();

    // If windows are open, toggle waybar to hidden (matches assumed visible state)
    if windows_opened {
        set_waybar_visible(false, &waybar_state);
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
            set_waybar_visible(current_visible, &waybar_state);
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

/// Checks if waybar process is currently running.
///
/// # Returns
///
/// Returns `true` if waybar process is found, `false` otherwise.
fn is_waybar_running() -> bool {
    Command::new("pgrep")
        .args(["-x", "waybar"])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
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

/// Checks if a toggle is needed based on desired and current visibility state.
///
/// This is logically equivalent to `visible != current_state` (XOR operation).
/// A toggle is needed when the desired state differs from the current state.
///
/// # Arguments
///
/// * `visible` - The desired visibility state
/// * `current_state` - The current visibility state
///
/// # Returns
///
/// Returns `true` if the states differ and a toggle is needed, `false` otherwise.
const fn needs_toggle(visible: bool, current_state: bool) -> bool {
    visible != current_state
}

/// Sets waybar visibility using signal-based approach.
///
/// Uses waybar's signal system with proper state tracking to avoid
/// sync issues. Always uses SIGUSR1 for toggling visibility.
///
/// # Arguments
///
/// * `visible` - Whether waybar should be visible
/// * `state` - Shared state for tracking waybar visibility state
///
/// # Panics
///
/// This function does not panic. All error cases, including poisoned mutexes
/// (when a thread panicked while holding the lock), are handled gracefully
/// by logging errors and returning early. Lock acquisition failures and signal
/// sending failures after all retries are also handled without panicking.
fn set_waybar_visible(visible: bool, state: &Arc<Mutex<WaybarState>>) {
    let Ok(mut state_guard) = state.lock() else {
        eprintln!("Failed to acquire waybar state lock");
        return;
    };

    // Check if toggle is needed
    let current_state = state_guard.is_visible;
    if !needs_toggle(visible, current_state) {
        return;
    }

    // Hold lock during signal sending to prevent race conditions
    // Release lock only during sleep operations to avoid blocking
    let mut signal_sent = false;
    for attempt in 1..=WAYBAR_SIGNAL_RETRY_COUNT {
        // Re-check state before sending to handle concurrent updates
        let current_state = state_guard.is_visible;
        if !needs_toggle(visible, current_state) {
            // Another thread already changed the state
            return;
        }

        drop(state_guard); // Release lock during command execution
        let command_result = Command::new("killall")
            .args(["-SIGUSR1", "waybar"])
            .output();

        // Re-acquire lock immediately after command
        state_guard = match state.lock() {
            Ok(guard) => guard,
            Err(_) => {
                eprintln!(
                    "Warning: Failed to acquire waybar state lock after sending signal. \
                     Waybar state may have changed but internal state not updated - \
                     potential state drift. Will retry on next call."
                );
                return;
            }
        };

        // Re-check state after re-acquiring lock to detect if another thread
        // changed the state while the lock was released
        let current_state = state_guard.is_visible;
        if !needs_toggle(visible, current_state) {
            // Another thread already changed the state while we were sending the signal
            return;
        }

        match command_result {
            Ok(output) => {
                if output.status.success() {
                    signal_sent = true;
                    // Update state immediately while lock is held to prevent race conditions
                    // where another thread checks state during the post-signal sleep
                    state_guard.is_visible = visible;
                    break;
                } else {
                    let exit_code = output.status.code().unwrap_or(-1);
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    eprintln!(
                        "Failed to send SIGUSR1 to waybar (attempt {}/{}): exit code {}, stderr: {}",
                        attempt,
                        WAYBAR_SIGNAL_RETRY_COUNT,
                        exit_code,
                        if stderr.is_empty() { "<none>" } else { &stderr }
                    );
                }
            }
            Err(e) => {
                eprintln!(
                    "Failed to send SIGUSR1 to waybar (attempt {}/{}): {e}",
                    attempt, WAYBAR_SIGNAL_RETRY_COUNT
                );
            }
        }

        if attempt < WAYBAR_SIGNAL_RETRY_COUNT {
            drop(state_guard); // Release lock during sleep
            thread::sleep(Duration::from_millis(WAYBAR_SIGNAL_RETRY_DELAY_MS));
            state_guard = match state.lock() {
                Ok(guard) => guard,
                Err(_) => {
                    eprintln!(
                        "Warning: Failed to acquire waybar state lock after sleep. \
                         Cannot continue retry loop. Will retry on next function call."
                    );
                    return;
                }
            };
        }
    }

    // Extra retry with longer delay if initial retries failed
    // This applies to both showing and hiding for consistent reliability
    if !signal_sent {
        // Re-check state before extra retry
        let current_state = state_guard.is_visible;
        if needs_toggle(visible, current_state) {
            drop(state_guard); // Release lock during sleep and command
            thread::sleep(Duration::from_millis(WAYBAR_EXTRA_RETRY_DELAY_MS));
            let command_result = Command::new("killall")
                .args(["-SIGUSR1", "waybar"])
                .output();

            // Re-acquire lock
            state_guard = match state.lock() {
                Ok(guard) => guard,
                Err(_) => {
                    eprintln!(
                        "Warning: Failed to acquire waybar state lock after extra retry. \
                         Waybar state may have changed but internal state not updated - \
                         potential state drift. Will retry on next call."
                    );
                    return;
                }
            };

            // Re-check state after re-acquiring lock to detect if another thread
            // changed the state while the lock was released
            let current_state = state_guard.is_visible;
            if !needs_toggle(visible, current_state) {
                // Another thread already changed the state while we were sending the signal
                return;
            }

            match command_result {
                Ok(output) => {
                    if output.status.success() {
                        signal_sent = true;
                        // Update state immediately while lock is held to prevent race conditions
                        // where another thread checks state during the post-signal sleep
                        state_guard.is_visible = visible;
                    } else {
                        let exit_code = output.status.code().unwrap_or(-1);
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        eprintln!(
                            "Failed to send SIGUSR1 to waybar (extra retry): exit code {}, stderr: {}",
                            exit_code,
                            if stderr.is_empty() { "<none>" } else { &stderr }
                        );
                    }
                }
                Err(e) => {
                    eprintln!("Failed to send SIGUSR1 to waybar (extra retry): {e}");
                }
            }
        }
    }

    drop(state_guard); // Release lock during final sleep
    thread::sleep(Duration::from_millis(WAYBAR_SIGNAL_POST_DELAY_MS));

    // Final safety check: verify state is correct after sleep
    // (State should already be updated immediately after signal send, but this
    // serves as a safety net in case another thread changed it during the sleep)
    if signal_sent {
        let Ok(mut state_guard) = state.lock() else {
            eprintln!(
                "Warning: Failed to acquire waybar state lock for final check. \
                 Signal was sent successfully but final state verification failed - \
                 potential state drift. Will retry on next call."
            );
            return;
        };
        // Final check to ensure state is still correct
        let current_state = state_guard.is_visible;
        if needs_toggle(visible, current_state) {
            state_guard.is_visible = visible;
        }
    } else {
        // Signal failed after all retries - state not updated, will retry on next call
        eprintln!(
            "Warning: Failed to send visibility signal to waybar after all retries. \
             State not updated - will retry on next visibility change."
        );
    }
}
