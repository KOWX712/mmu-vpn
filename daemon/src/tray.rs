use std::sync::{Arc, Mutex};

use muda::{Menu, MenuItem};
use tray_icon::{Icon, TrayIconBuilder, TrayIconEvent};
use tao::event::{Event, StartCause};
use tao::event_loop::{ControlFlow, EventLoopBuilder};

use crate::vpn::{State, VpnDaemon};

const ICON_DISCONNECTED: &[u8] = include_bytes!("../network-vpn-disconnected.png");
const ICON_CONNECTING: &[u8] = include_bytes!("../network-vpn-acquiring.png");
const ICON_CONNECTED: &[u8] = include_bytes!("../network-vpn.png");

fn icon_from_png(data: &[u8]) -> Icon {
    let decoder = png::Decoder::new(std::io::Cursor::new(data));
    let mut reader = decoder.read_info().unwrap();
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).unwrap();
    buf.truncate(info.buffer_size());
    Icon::from_rgba(buf, info.width, info.height).unwrap()
}

fn icon_for_state(state: State) -> Icon {
    let data = match state {
        State::Disconnected => ICON_DISCONNECTED,
        State::Connecting => ICON_CONNECTING,
        State::Connected => ICON_CONNECTED,
    };
    icon_from_png(data)
}

pub fn run(daemon: Arc<Mutex<VpnDaemon>>, auto_connect: bool) {
    let event_loop = EventLoopBuilder::new().build();

    let menu = Menu::new();
    let start_item = MenuItem::new("Start VPN", true, None);
    let stop_item = MenuItem::new("Stop VPN", true, None);
    let quit_item = MenuItem::new("Quit", true, None);
    stop_item.set_enabled(false);
    menu.append_items(&[&start_item, &stop_item, &quit_item]).unwrap();

    let mut tray_icon = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("MMU VPN — Disconnected")
        .with_icon(icon_for_state(State::Disconnected))
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

        // Drain all pending menu events
        while let Ok(event) = menu_channel.try_recv() {
            if event.id == start_item.id() {
                println!("[mmuvpn] Start VPN clicked");
                daemon.lock().unwrap().start();
                let state = daemon.lock().unwrap().state;
                update_tray(&mut tray_icon, state);
                update_menu(&start_item, &stop_item, state);
            } else if event.id == stop_item.id() {
                println!("[mmuvpn] Stop VPN clicked");
                daemon.lock().unwrap().stop();
                update_tray(&mut tray_icon, State::Disconnected);
                update_menu(&start_item, &stop_item, State::Disconnected);
            } else if event.id == quit_item.id() {
                println!("[mmuvpn] Quit clicked");
                daemon.lock().unwrap().stop();
                *control_flow = ControlFlow::Exit;
                return;
            }
        }

        // Drain tray icon events
        while let Ok(event) = tray_channel.try_recv() {
            println!("[mmuvpn] Tray event: {:?}", event);
        }

        if let Event::MainEventsCleared | Event::RedrawEventsCleared = event {
            let mut d = daemon.lock().unwrap();
            let old = d.state;
            d.check_alive();
            let new = d.state;
            if new != old {
                println!("[mmuvpn] State changed: {:?} -> {:?}", old, new);
                update_tray(&mut tray_icon, new);
                update_menu(&start_item, &stop_item, new);
            }
        }
    });
}

fn update_tray(tray: &mut tray_icon::TrayIcon, state: State) {
    let _ = tray.set_icon(Some(icon_for_state(state)));
    let _ = tray.set_tooltip(Some(format!("MMU VPN — {}", state.label())));
}

fn update_menu(start: &MenuItem, stop: &MenuItem, state: State) {
    let active = state == State::Connected || state == State::Connecting;
    start.set_enabled(!active);
    stop.set_enabled(active);
}
