use std::sync::{Arc, Mutex};

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

fn icon_for_state(state: State, dark: bool) -> Icon {
    let data = match state {
        State::Disconnected => SVG_DISCONNECTED,
        State::Connecting => SVG_CONNECTING,
        State::Connected => SVG_CONNECTED,
    };
    svg_to_icon(data, dark)
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
        .with_tooltip("MMU VPN — Disconnected")
        .with_icon(icon_for_state(State::Disconnected, dark))
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
            update_menu(&start_item, &stop_item, daemon.lock().unwrap().state);
        }

        while let Ok(event) = menu_channel.try_recv() {
            if event.id == start_item.id() {
                println!("[mmuvpn] Start VPN clicked");
                daemon.lock().unwrap().start();
                let state = daemon.lock().unwrap().state;
                update_tray(&mut tray_icon, state, dark);
                update_menu(&start_item, &stop_item, state);
            } else if event.id == stop_item.id() {
                println!("[mmuvpn] Stop VPN clicked");
                daemon.lock().unwrap().stop();
                update_tray(&mut tray_icon, State::Disconnected, dark);
                update_menu(&start_item, &stop_item, State::Disconnected);
            } else if event.id == quit_item.id() {
                println!("[mmuvpn] Quit clicked");
                daemon.lock().unwrap().stop();
                *control_flow = ControlFlow::Exit;
                return;
            }
        }

        while let Ok(event) = tray_channel.try_recv() {
            println!("[mmuvpn] Tray event: {:?}", event);
        }

        while let Some(cmd) = instance.accept_pending() {
            match cmd.as_str() {
                "start" => {
                    println!("[mmuvpn] IPC: start VPN");
                    daemon.lock().unwrap().start();
                    let state = daemon.lock().unwrap().state;
                    update_tray(&mut tray_icon, state, dark);
                    update_menu(&start_item, &stop_item, state);
                }
                "stop" => {
                    println!("[mmuvpn] IPC: stop VPN");
                    daemon.lock().unwrap().stop();
                    update_tray(&mut tray_icon, State::Disconnected, dark);
                    update_menu(&start_item, &stop_item, State::Disconnected);
                }
                "quit" => {
                    println!("[mmuvpn] IPC: quit daemon");
                    daemon.lock().unwrap().stop();
                    *control_flow = ControlFlow::Exit;
                    return;
                }
                other => {
                    eprintln!("[mmuvpn] Unknown IPC command: {}", other);
                }
            }
        }

        if let Event::MainEventsCleared | Event::RedrawEventsCleared = event {
            let new_dark = is_dark();
            if new_dark != dark {
                dark = new_dark;
                println!("[mmuvpn] Theme changed: dark={}", dark);
                let state = daemon.lock().unwrap().state;
                update_tray(&mut tray_icon, state, dark);
            }

            let mut d = daemon.lock().unwrap();
            let old = d.state;
            d.check_alive();
            let new = d.state;
            if new != old {
                println!("[mmuvpn] State changed: {:?} -> {:?}", old, new);
                update_tray(&mut tray_icon, new, dark);
                update_menu(&start_item, &stop_item, new);
            }
        }
    });
}

fn update_tray(tray: &mut tray_icon::TrayIcon, state: State, dark: bool) {
    let _ = tray.set_icon(Some(icon_for_state(state, dark)));
    let _ = tray.set_tooltip(Some(format!("MMU VPN — {}", state.label())));
}

fn update_menu(start: &MenuItem, stop: &MenuItem, state: State) {
    let active = state == State::Connected || state == State::Connecting;
    start.set_enabled(!active);
    stop.set_enabled(active);
}
