use clap::{Parser, ValueEnum};
use serde::Deserialize;
use std::{
    collections::HashMap,
    sync::mpsc::{self},
};

use crate::{hyprland::Workspace, waybar::WaybarProcess};

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

    hyprland::spawn_mouse_position_updater(tx.clone(), args.clone());
    hyprland::spawn_window_event_listener(tx.clone());

    tx.send(Event::CursorTop(String::default(), false)).ok();

    let mut monitors_cond: HashMap<String, MonitorConditions> = HashMap::new();
    let waybar_processes: Vec<WaybarProcess> = waybar::waybar_processes();

    let (Some(monitors), Some(clients)) = (hyprland::get_monitors(), hyprland::get_clients())
    else {
        println!("Could not communicate with hyprland");
        return;
    };

    for m in monitors {
        let has_windows = hyprland::check_windows_workspace(&m.workspace, &clients);

        monitors_cond.insert(
            m.name,
            MonitorConditions {
                has_cursor_edge: false,
                has_windows,
                waybar_visible: !has_windows,
            },
        );
    }

    for event in rx {
        match event {
            Event::CursorTop(output, val) => {
                if let Some(conditions) = monitors_cond.get_mut(&output) {
                    conditions.has_cursor_edge = val
                }
            }

            Event::WindowsOpened(output, val) => {
                if let Some(conditions) = monitors_cond.get_mut(&output) {
                    conditions.has_windows = val
                }
            }
        }

        for (output, condition) in monitors_cond.iter_mut() {
            let current_visible: bool = match args.always_hidden {
                true => condition.has_cursor_edge,
                false if condition.has_cursor_edge => true,
                false => !condition.has_windows,
            };

            if current_visible != condition.waybar_visible {
                for process in waybar_processes.iter() {
                    if process.output.as_ref().is_some_and(|o| *o == *output)
                        || process.output.is_none()
                    {
                        set_waybar_visible(process.pid, current_visible);
                    }
                }
            }

            condition.waybar_visible = current_visible;
        }
    }
}

#[derive(Debug)]
enum Event {
    CursorTop(String, bool),
    WindowsOpened(String, bool),
}

struct MonitorConditions {
    has_cursor_edge: bool,
    has_windows: bool,
    waybar_visible: bool,
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
    name: String,
    #[serde(rename = "activeWorkspace")]
    workspace: Workspace,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    scale: f64,
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
