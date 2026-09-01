use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::sync::Arc;
use std::{
    io::{Read, Write},
    os::unix::net::UnixStream,
    sync::mpsc,
    thread,
};

use serde::Deserialize;
use std::{sync::mpsc::Sender, time::Duration};

use super::Event;

use crate::{
    Conditions, CursorPos, EventFlag, MOUSE_REFRESH_DELAY_MS, Monitor, PIXEL_THRESHOLD,
    PIXEL_THRESHOLD_SECONDARY, Side, WaybarInstance,
};

#[derive(Deserialize)]
pub struct Client {
    workspace: Workspace,
}

#[derive(Deserialize)]
pub struct Workspace {
    id: i32,
}

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
pub fn get_monitors() -> Option<Vec<Monitor>> {
    serde_json::from_str(&hypr_query("j/monitors")?).ok()
}

/// Returns true of a window or more is open
pub fn check_windows(instances: &HashMap<i32, WaybarInstance>) -> Vec<Event> {
    let mut result: Vec<Event> = Vec::new();

    let (Some(monitors), Some(clients)) = (get_monitors(), get_clients()) else {
        return result;
    };

    for (pid, instance) in instances.iter() {
        let has_windows = match &instance.process.output {
            Some(name) => monitors
                .iter()
                .find(|m| m.name == *name)
                .is_some_and(|m| check_windows_workspace(&m.workspace, &clients)),
            // A bar with no `output` spans every monitor: occupied if any is
            None => monitors
                .iter()
                .any(|m| check_windows_workspace(&m.workspace, &clients)),
        };

        result.push(Event {
            pid: *pid,
            flag: EventFlag::WindowsOpened(has_windows),
        });
    }

    result
}

/// All open windows across every monitor and workspace
pub fn get_clients() -> Option<Vec<Client>> {
    serde_json::from_str(&hypr_query("j/clients")?).ok()
}

/// Returns true if at least one window is present on the given workspace
pub fn check_windows_workspace(workspace: &Workspace, clients: &Vec<Client>) -> bool {
    clients.iter().any(|c| c.workspace.id == workspace.id)
}

/// Watches the compositor event stream and re-checks the window count whenever something
/// might have changed it.
pub fn spawn_window_event_listener(
    tx: mpsc::Sender<Event>,
    instances: Arc<HashMap<i32, WaybarInstance>>,
) {
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
            if line.contains("openwindow")
                || line.contains("workspace")
                || line.contains("closewindow")
                || line.contains("movewindow")
            {
                for event in check_windows(&instances) {
                    tx.send(event).ok();
                }
            }
        }
    });
}

/// Keeps track of the mouse position at a constant polling rate (`100 ms` by default)
/// and checks its distance from the desired edge.
///
/// Sends events to the main thread when the condition changes.
pub fn spawn_mouse_position_updater(
    tx: Sender<Event>,
    waybar_instances: Arc<HashMap<i32, WaybarInstance>>,
) {
    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_millis(MOUSE_REFRESH_DELAY_MS));

            let (Some(pos), Some(monitors)) = (get_cursor_pos(), get_monitors()) else {
                continue;
            };
            // Monitor with the cursor in it
            let Some(monitor) = monitors.iter().find(|m| m.contains(&pos)) else {
                continue;
            };

            for instance in waybar_instances.values() {
                // Multi-monitor fix: Find which monitor the cursor is currently on
                let mut cond = instance.conditions.lock().unwrap();

                let is_cursor_edge = resolve_cursor_edge(&pos, monitor, instance, &cond);
                if is_cursor_edge != cond.has_cursor_edge {
                    tx.send(Event {
                        pid: instance.process.pid,
                        flag: EventFlag::CursorTop(is_cursor_edge),
                    })
                    .ok();
                }
                cond.has_cursor_edge = is_cursor_edge;
            }
        }
    });
}

/// Returns true if the cursor is within the instance's workspace
/// and if it's close enough to the side.
fn resolve_cursor_edge(
    pos: &CursorPos,
    active_monitor: &Monitor,
    instance: &WaybarInstance,
    conditions: &Conditions,
) -> bool {
    match instance
        .process
        .output
        .as_ref()
        .is_none_or(|m| *m == active_monitor.name)
    {
        // Cursor is not on the processes's workspace
        false => false,
        true => {
            let side = instance.process.side;
            let threshold = match conditions.has_cursor_edge {
                true => PIXEL_THRESHOLD_SECONDARY,
                false => PIXEL_THRESHOLD,
            };
            distance_from_edge(pos, active_monitor, side) <= threshold
        }
    }
}

/// Returns the distance in pixels from the cursor to the desired edge.
fn distance_from_edge(pos: &CursorPos, active_monitor: &Monitor, side: Side) -> i32 {
    // Scaling fix: Mouse position in impl Monitor
    match side {
        Side::Top => pos.y - active_monitor.y,
        Side::Bottom => (active_monitor.y + active_monitor.logical_height()) - pos.y,
        Side::Left => pos.x - active_monitor.x,
        Side::Right => (active_monitor.x + active_monitor.logical_width()) - pos.x,
    }
}
