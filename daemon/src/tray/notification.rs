use crate::vpn::Notification;
use notify_rust::Notification as DesktopNotification;

pub fn show_notification(notif: &Notification) {
    match notif {
        Notification::Connected => {
            let _ = DesktopNotification::new()
                .summary("MMU VPN")
                .body("Connected successfully")
                .icon("network-vpn")
                .timeout(3000)
                .show();
        }
        Notification::Error(msg) => {
            let _ = DesktopNotification::new()
                .summary("MMU VPN Connection Failed")
                .body(msg)
                .icon("dialog-error")
                .timeout(0)
                .show();
        }
        Notification::CampusDetected => {
            let _ = DesktopNotification::new()
                .summary("MMU VPN")
                .body("Already on campus network - VPN may not be needed")
                .icon("dialog-information")
                .timeout(5000)
                .show();
        }
    }
}
