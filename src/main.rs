use clap::Parser;
use serde::Deserialize;
use std::{
    collections::HashMap,
    sync::mpsc::{self},
};

use crate::{
    hyprland::{
        Client, Workspace, check_windows_workspace, get_clients, get_cursor_pos, get_monitors,
        resolve_cursor_edge,
    },
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

    if args.side.is_some() {
        let msg = "waybar_auto_hide: --side is deprecated and ignored. \
                   The side is now read from each waybar config's \"position\" field.";
        eprintln!("warning: {msg}");
        hyprland::notify(0, 10000, msg); // 0 = warning
    }

    let (tx, rx) = mpsc::channel::<Event>();

    let mut instances: HashMap<i32, WaybarInstance> = HashMap::new();
    for process in waybar::waybar_processes() {
        instances.insert(process.pid, WaybarInstance::new(process));
    }

    hyprland::spawn_mouse_position_updater(tx.clone());
    hyprland::hyprland_events_listener(tx.clone());

    // "World" data getting updated by events
    let mut monitors = get_monitors().unwrap_or_default();
    let mut cursor_pos = get_cursor_pos().unwrap_or_default();
    let mut clients: Vec<Client> = get_clients().unwrap_or_default();

    for event in rx {
        match event {
            Event::CursorUpdate(updated_pos) => cursor_pos = updated_pos,
            Event::WindowsUpdate(updated_clients) => clients = updated_clients,
            Event::MonitorUpdates(updated_monitors) => monitors = updated_monitors,
        }

        for instance in instances.values_mut() {
            update(instance, &cursor_pos, &monitors, &clients, &args);
        }
    }
}

/// Recomputes one instance's state from the current world and signals Waybar if it changed.
fn update(
    instance: &mut WaybarInstance,
    cursor_pos: &CursorPos,
    monitors: &[Monitor],
    clients: &Vec<Client>,
    args: &Args,
) {
    // Monitor with the cursor in it
    let cursor_monitor = monitors.iter().find(|m| m.contains(cursor_pos));

    // Read before the write: resolve_cursor_edge needs the previous value for hysteresis
    let cursor_edge =
        cursor_monitor.is_some_and(|monitor| resolve_cursor_edge(cursor_pos, monitor, instance));
    instance.cursor_edge = cursor_edge;

    instance.windows = match &instance.process.output {
        Some(name) => monitors
            .iter()
            .find(|m| m.name == *name)
            .is_some_and(|m| check_windows_workspace(&m.workspace, clients)),
        // A bar with no `output` spans every monitor: occupied if any is
        None => monitors
            .iter()
            .any(|m| check_windows_workspace(&m.workspace, clients)),
    };

    let current_visible: bool = match args.always_hidden {
        true => instance.cursor_edge,
        false if instance.cursor_edge => true,
        false => !instance.windows,
    };

    if current_visible != instance.visible {
        set_waybar_visible(instance.process.pid, current_visible);
        instance.visible = current_visible;
    }
}

enum Event {
    CursorUpdate(CursorPos),
    WindowsUpdate(Vec<Client>),
    MonitorUpdates(Vec<Monitor>),
}

struct WaybarInstance {
    process: WaybarProcess,
    pub cursor_edge: bool,
    pub windows: bool,
    pub visible: bool,
}

impl WaybarInstance {
    fn new(process: WaybarProcess) -> Self {
        Self {
            process,
            cursor_edge: false,
            windows: false,
            visible: true,
        }
    }
}

/// Uses direct syscalls to signal Waybar
fn set_waybar_visible(pid: i32, visible: bool) -> bool {
    let signal = if visible { 12 } else { 10 }; // SIGUSR2 (show), SIGUSR1 (hide)
    unsafe { libc::kill(pid, signal) == 0 }
}

#[derive(Deserialize, Default)]
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

    /// Deprecated and ignored: the side is now read from each waybar config's `position`.
    #[arg(long, hide = true)]
    side: Option<String>,
}
