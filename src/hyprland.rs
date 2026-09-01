use std::io::{BufRead, BufReader};
use std::{
    io::{Read, Write},
    os::unix::net::UnixStream,
    sync::mpsc,
    thread,
};

use std::{sync::mpsc::Sender, time::Duration};

use serde::Deserialize;

use super::Event;

use crate::{
    Args, CursorPos, MOUSE_REFRESH_DELAY_MS, Monitor, PIXEL_THRESHOLD, PIXEL_THRESHOLD_SECONDARY,
    Side,
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
pub fn check_windows() -> Vec<Event> {
    let mut result: Vec<Event> = Vec::new();

    let (Some(monitors), Some(clients)) = (get_monitors(), get_clients()) else {
        return result;
    };

    for monitor in monitors.iter() {
        let event = Event::WindowsOpened(
            monitor.name.clone(),
            check_windows_workspace(&monitor.workspace, &clients),
        );

        result.push(event);
    }

    return result;
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
            if line.contains("openwindow")
                || line.contains("workspace")
                || line.contains("closewindow")
                || line.contains("movewindow")
            {
                for event in check_windows() {
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
pub fn spawn_mouse_position_updater(tx: Sender<Event>, args: Args) {
    thread::spawn(move || {
        let mut previous_state = false;
        loop {
            thread::sleep(Duration::from_millis(MOUSE_REFRESH_DELAY_MS));
            let (Some(pos), Some(monitors)) = (get_cursor_pos(), get_monitors()) else {
                continue;
            };

            // Multi-monitor fix: Find which monitor the cursor is currently on
            // Scaling fix: Mouse position in impl Monitor
            let Some(active_monitor) = monitors.iter().find(|m| m.contains(&pos)) else {
                continue;
            };

            // Scaling fix: Scaling calculation in impl Monitor
            let distance_from_edge = match args.side {
                Side::Top => pos.y - active_monitor.y,
                Side::Bottom => (active_monitor.y + active_monitor.logical_height()) - pos.y,
                Side::Left => pos.x - active_monitor.x,
                Side::Right => (active_monitor.x + active_monitor.logical_width()) - pos.x,
            };

            let threshold = match previous_state {
                true => PIXEL_THRESHOLD_SECONDARY,
                false => PIXEL_THRESHOLD,
            };

            let is_cursor_edge = distance_from_edge <= threshold;

            if is_cursor_edge != previous_state {
                tx.send(Event::CursorTop(
                    active_monitor.name.clone(),
                    is_cursor_edge,
                ))
                .ok();
            }
            previous_state = is_cursor_edge;
        }
    });
}
