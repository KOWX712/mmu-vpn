use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use muda::{Menu, MenuItem};
use tray_icon::{Icon, TrayIconBuilder, TrayIconEvent};
use tao::event::{Event, StartCause};
use tao::event_loop::{ControlFlow, EventLoopBuilder};

use crate::SingleInstance;
use crate::vpn::{State, VpnDaemon};

const SVG_DISCONNECTED: &[u8] =
    include_bytes!("../icons/hicolor/scalable/status/network-vpn-disconnected.svg");
const SVG_CONNECTING: &[u8] =
    include_bytes!("../icons/hicolor/scalable/status/network-vpn-acquiring.svg");
const SVG_CONNECTED: &[u8] =
    include_bytes!("../icons/hicolor/scalable/status/network-vpn.svg");
const SVG_ERROR: &[u8] =
    include_bytes!("../icons/hicolor/scalable/status/network-vpn-disconnected.svg");

enum Cmd {
    Start,
    Stop,
    Quit,
}

fn is_dark() -> bool {
    matches!(dark_light::detect(), Ok(dark_light::Mode::Dark))
}

fn svg_to_icon(svg_data: &[u8], dark: bool) -> Icon {
    let color = if dark { "#e0e0e0" } else { "#2d2d2d" };
    let svg_string = String::from_utf8_lossy(svg_data).replace("#dedede", color);

    let opt = usvg::Options::default();
    let tree = usvg::Tree::from_data(svg_string.as_bytes(), &opt).unwrap();
    let svg_size = tree.size().to_int_size();
    let scale = 4;
    let w = svg_size.width() * scale;
    let h = svg_size.height() * scale;
    let mut pixmap = tiny_skia::Pixmap::new(w, h).unwrap();
    let transform = tiny_skia::Transform::from_scale(scale as f32, scale as f32);
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    Icon::from_rgba(pixmap.data().to_vec(), w, h).unwrap()
}

fn icon_for_state(state: &State, dark: bool) -> Icon {
    let data = match state {
        State::Disconnected => SVG_DISCONNECTED,
        State::Connecting => SVG_CONNECTING,
        State::Connected => SVG_CONNECTED,
        State::Error(_) => SVG_ERROR,
    };
    svg_to_icon(data, dark)
}

fn wait_for_vpn_stop() {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        let running = Command::new("pgrep")
            .arg("openfortivpn")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if !running {
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }
}

pub fn run(daemon: Arc<Mutex<VpnDaemon>>, auto_connect: bool, instance: SingleInstance) {
    let event_loop = EventLoopBuilder::new().build();

    let menu = Menu::new();
    let start_item = MenuItem::new("Start VPN", true, None);
    let stop_item = MenuItem::new("Stop VPN", true, None);
    let quit_item = MenuItem::new("Quit", true, None);
    stop_item.set_enabled(false);
    menu.append_items(&[&start_item, &stop_item, &quit_item]).unwrap();

    let mut dark = is_dark();
    let mut tray_icon = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip(format!("MMU VPN — {}", daemon.lock().unwrap().state.label()))
        .with_icon(icon_for_state(&State::Disconnected, dark))
        .build()
        .expect("Failed to create tray icon");

    start_item.set_enabled(true);
    stop_item.set_enabled(false);

    if auto_connect {
        daemon.lock().unwrap().start();
    }

    let menu_channel = muda::MenuEvent::receiver();
    let tray_channel = TrayIconEvent::receiver();

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Poll;

        if let Event::NewEvents(StartCause::Init) = event {
            let state = &daemon.lock().unwrap().state;
            update_menu(&start_item, &stop_item, state);
        }

        while let Ok(event) = menu_channel.try_recv() {
            let cmd = if event.id == start_item.id() {
                Some(Cmd::Start)
            } else if event.id == stop_item.id() {
                Some(Cmd::Stop)
            } else if event.id == quit_item.id() {
                Some(Cmd::Quit)
            } else {
                None
            };
            if let Some(cmd) = cmd {
                if handle_cmd(cmd, &mut tray_icon, &start_item, &stop_item, &daemon, dark, control_flow) {
                    return;
                }
            }
        }

        while let Ok(event) = tray_channel.try_recv() {
            println!("[mmuvpn] Tray event: {:?}", event);
        }

        while let Some(cmd) = instance.accept_pending() {
            let cmd = match cmd.as_str() {
                "start" => Some(Cmd::Start),
                "stop" => Some(Cmd::Stop),
                "quit" => Some(Cmd::Quit),
                other => {
                    eprintln!("[mmuvpn] Unknown IPC command: {}", other);
                    None
                }
            };
            if let Some(cmd) = cmd {
                if handle_cmd(cmd, &mut tray_icon, &start_item, &stop_item, &daemon, dark, control_flow) {
                    return;
                }
            }
        }

        if let Event::MainEventsCleared | Event::RedrawEventsCleared = event {
            let new_dark = is_dark();
            if new_dark != dark {
                dark = new_dark;
                println!("[mmuvpn] Theme changed: dark={}", dark);
                let state = &daemon.lock().unwrap().state;
                update_tray(&mut tray_icon, state, dark);
            }

            let mut d = daemon.lock().unwrap();
            let old = d.state.clone();
            d.check_alive();
            let new = d.state.clone();
            if new != old {
                println!("[mmuvpn] State changed: {:?} -> {:?}", old, new);
                update_tray(&mut tray_icon, &new, dark);
                update_menu(&start_item, &stop_item, &new);
            }
        }
    });
}

fn handle_cmd(
    cmd: Cmd,
    tray: &mut tray_icon::TrayIcon,
    start: &MenuItem,
    stop: &MenuItem,
    daemon: &Arc<Mutex<VpnDaemon>>,
    dark: bool,
    control_flow: &mut ControlFlow,
) -> bool {
    match cmd {
        Cmd::Start => {
            println!("[mmuvpn] Start VPN");
            daemon.lock().unwrap().start();
            let state = daemon.lock().unwrap().state.clone();
            update_tray(tray, &state, dark);
            update_menu(start, stop, &state);
        }
        Cmd::Stop => {
            println!("[mmuvpn] Stop VPN");
            daemon.lock().unwrap().stop();
            let state = daemon.lock().unwrap().state.clone();
            update_tray(tray, &state, dark);
            update_menu(start, stop, &state);
        }
        Cmd::Quit => {
            println!("[mmuvpn] Quit");
            // stop() already kills openfortivpn + restores DNS in one admin prompt on macOS.
            daemon.lock().unwrap().stop();
            wait_for_vpn_stop();
            *control_flow = ControlFlow::Exit;
            return true;
        }
    }
    false
}

fn update_tray(tray: &mut tray_icon::TrayIcon, state: &State, dark: bool) {
    let _ = tray.set_icon(Some(icon_for_state(state, dark)));
    let _ = tray.set_tooltip(Some(format!("MMU VPN — {}", state.label())));
}

fn update_menu(start: &MenuItem, stop: &MenuItem, state: &State) {
    let active = state.is_active();
    start.set_enabled(!active);
    stop.set_enabled(active);
}
