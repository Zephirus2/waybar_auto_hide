use clap::Parser;
use serde::Deserialize;
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        mpsc::{self},
    },
};

use crate::{
    hyprland::{Workspace, check_windows},
    waybar::{Side, WaybarProcess},
};

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

    let mut instances: HashMap<i32, WaybarInstance> = HashMap::new();
    for process in waybar::waybar_processes() {
        instances.insert(
            process.pid,
            WaybarInstance {
                conditions: State::default().into(),
                process: process.clone(),
            },
        );
    }

    let instances: Arc<HashMap<i32, WaybarInstance>> = Arc::from(instances);

    for event in check_windows(&instances) {
        tx.send(event).ok();
    }

    hyprland::spawn_mouse_position_updater(tx.clone(), instances.clone());
    hyprland::spawn_window_event_listener(tx.clone(), instances.clone());

    for event in rx {
        let Some(instance) = instances.get(&event.pid) else {
            continue;
        };

        let mut condition = instance.conditions.lock().unwrap();
        match event.flag {
            EventFlag::CursorEdge(val) => condition.cursor_edge = val,
            EventFlag::WindowsOpen(val) => condition.windows = val,
        }

        let current_visible: bool = match args.always_hidden {
            true => condition.cursor_edge,
            false if condition.cursor_edge => true,
            false => !condition.windows,
        };

        if current_visible != condition.visible {
            set_waybar_visible(instance.process.pid, current_visible);
        }
        condition.visible = current_visible;
    }
}

#[derive(Debug)]
enum EventFlag {
    CursorEdge(bool),
    WindowsOpen(bool),
}

pub struct Event {
    pid: i32,
    flag: EventFlag,
}

struct WaybarInstance {
    conditions: Mutex<State>,
    process: WaybarProcess,
}

#[derive(Default, Clone, Copy)]
pub struct State {
    pub cursor_edge: bool,
    pub windows: bool,
    pub visible: bool,
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
}
