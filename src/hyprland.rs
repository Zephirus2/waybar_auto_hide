use std::io::{BufRead, BufReader};
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
    CursorPos, MOUSE_REFRESH_DELAY_MS, Monitor, PIXEL_THRESHOLD, PIXEL_THRESHOLD_SECONDARY, Side,
    WaybarInstance,
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

pub fn get_cursor_pos() -> Option<CursorPos> {
    serde_json::from_str(&hypr_query("j/cursorpos")?).ok()
}

/// All connected monitors, each with its position in the global layout
pub fn get_monitors() -> Option<Vec<Monitor>> {
    serde_json::from_str(&hypr_query("j/monitors")?).ok()
}

/// All open windows across every monitor and workspace
pub fn get_clients() -> Option<Vec<Client>> {
    serde_json::from_str(&hypr_query("j/clients")?).ok()
}

/// Returns true if at least one window is present on the given workspace
pub fn check_windows_workspace(workspace: &Workspace, clients: &[Client]) -> bool {
    clients.iter().any(|c| c.workspace.id == workspace.id)
}

/// Watches the compositor event stream and send updates to the main thread
pub fn hyprland_events_listener(tx: mpsc::Sender<Event>) {
    thread::spawn(move || {
        let Some(reader) = connect_hypr_socket() else {
            panic!("could not connect to hyprland socket")
        };

        for line in reader.lines().map_while(Result::ok) {
            // Clients related updates (windows opened, closed or moved)
            if line.contains("openwindow")
                || line.contains("closewindow")
                || line.contains("movewindow")
            {
                let Some(clients) = get_clients() else {
                    continue;
                };

                tx.send(Event::WindowsUpdate(clients)).ok();
            }

            // Monitor related updates (workspace change, monitor plugged in etc)
            if line.contains("monitor") || line.contains("workspace") {
                let Some(monitors) = get_monitors() else {
                    continue;
                };

                tx.send(Event::MonitorUpdates(monitors)).ok();
            }
        }
    });
}

fn connect_hypr_socket() -> Option<BufReader<UnixStream>> {
    let socket_path = format!(
        "{}/hypr/{}/.socket2.sock",
        std::env::var("XDG_RUNTIME_DIR").unwrap(),
        std::env::var("HYPRLAND_INSTANCE_SIGNATURE").unwrap()
    );
    let stream = UnixStream::connect(&socket_path).ok()?;
    let reader = BufReader::new(stream);

    Some(reader)
}

/// Sends the mouse position to the main thread at a constant polling rate
pub fn spawn_mouse_position_updater(tx: Sender<Event>) {
    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_millis(MOUSE_REFRESH_DELAY_MS));

            let Some(pos) = get_cursor_pos() else {
                continue;
            };

            tx.send(Event::CursorUpdate(pos)).ok();
        }
    });
}

/// Returns true if the cursor is within the instance's workspace
/// and if it's close enough to the side.
pub fn resolve_cursor_edge(
    pos: &CursorPos,
    active_monitor: &Monitor,
    instance: &WaybarInstance,
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
            let threshold = match instance.cursor_edge {
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
