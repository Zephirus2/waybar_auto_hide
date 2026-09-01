use clap::{Parser, ValueEnum};
use serde::Deserialize;
use std::sync::mpsc::{self};

mod hyprland;
mod waybar;

// The distance from the top at which the bar will activate
const PIXEL_THRESHOLD: i32 = 3;

// The distance from the top at which the bar will hide again.
const PIXEL_THRESHOLD_SECONDARY: i32 = 50;
const MOUSE_REFRESH_DELAY_MS: u64 = 100;

fn main() {
    let args = Args::parse();
    let (tx, rx) = mpsc::channel::<Event>();

    let mut cursor_top: bool = false;
    let mut windows_opened: bool = hyprland::check_windows();
    let mut last_visibility: bool = !windows_opened;

    hyprland::spawn_mouse_position_updater(tx.clone(), args.clone());
    hyprland::spawn_window_event_listener(tx.clone());

    tx.send(Event::CursorTop(false)).ok();
    tx.send(Event::WindowsOpened(windows_opened)).ok();

    // Cache Waybar PID to avoid repeated lookups
    let mut waybar_pid: Option<Vec<i32>> = waybar::find_waybar_pids();

    for event in rx {
        match event {
            Event::CursorTop(val) => cursor_top = val,
            Event::WindowsOpened(val) => windows_opened = val,
        }

        let current_visible = match args.always_hidden {
            true => cursor_top,
            false => {
                if cursor_top {
                    true
                } else {
                    !windows_opened
                }
            }
        };

        if current_visible != last_visibility {
            // Refreshes PID if it was lost or not found yet
            if waybar_pid.is_none() {
                waybar_pid = waybar::find_waybar_pids();
                continue;
            }

            if let Some(pid) = &waybar_pid {
                if !set_waybar_visible(pid[0], current_visible) {
                    // If signal fails, Waybar might have restarted
                    waybar_pid = waybar::find_waybar_pids();
                    if let Some(new_pid) = &waybar_pid {
                        set_waybar_visible(new_pid[0], current_visible);
                    }
                }
            }
        }
        last_visibility = current_visible
    }
}

#[derive(Debug)]
enum Event {
    CursorTop(bool),
    WindowsOpened(bool),
}

/// Uses direct syscalls to signal Waybar
fn set_waybar_visible(pid: i32, visible: bool) -> bool {
    let signal = if visible { 12 } else { 10 }; // SIGUSR2 (show), SIGUSR1 (hide)
    unsafe { libc::kill(pid, signal) == 0 }
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
    scale: f64, // Scaling fix: Get scaling factor
}

// Scaling fix: impl to fix scaling and check for cursor out of main loop
impl Monitor {
    fn logical_width(&self) -> i32 {
        (self.width as f64 / self.scale) as i32
    }

    fn logical_height(&self) -> i32 {
        (self.height as f64 / self.scale) as i32
    }

    fn contains(&self, pos: &CursorPos) -> bool {
        pos.x >= self.x
            && pos.x < self.x + self.logical_width()
            && pos.y >= self.y
            && pos.y < self.y + self.logical_height()
    }
}

#[derive(Parser, Clone)]
struct Args {
    /// If set, the bar will always hide when the cursor is not at the top
    #[arg(long)]
    always_hidden: bool,
    /// Which side of the screen the bar is located on. Beware that doesn't work well with multiple monitors.
    #[arg(long, value_enum, default_value = "top")]
    side: Side,
}

#[derive(Debug, Clone, Copy, Deserialize, ValueEnum)]
enum Side {
    Top,
    Left,
    Right,
    Bottom,
}

impl Default for Side {
    fn default() -> Self {
        Side::Top
    }
}
