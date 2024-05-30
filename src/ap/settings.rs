use std::{collections::HashMap, net::Ipv4Addr};

use futures::{pin_mut, stream, StreamExt};
use rusty_network_manager::{DeviceProxy, IP4ConfigProxy, NetworkManagerProxy};
use tracing::{debug, info};
use zbus::{
    zvariant::{ObjectPath, OwnedObjectPath, OwnedValue},
    Connection,
};

pub fn make_arguments_for_ap(
    ssid: &str,
) -> HashMap<&str, HashMap<&str, zbus::zvariant::Value<'_>>> {
    let mut settings: HashMap<&str, HashMap<&str, zbus::zvariant::Value<'_>>> = HashMap::new();

    let mut wireless: HashMap<&str, zbus::zvariant::Value<'_>> = HashMap::new();
    wireless.insert("ssid", zbus::zvariant::Value::new(ssid.as_bytes()));
    wireless.insert("mode", zbus::zvariant::Value::new("ap"));
    wireless.insert("band", zbus::zvariant::Value::new("bg"));
    wireless.insert("hidden", zbus::zvariant::Value::new(false));
    settings.insert("802-11-wireless", wireless);

    let mut ipv4: HashMap<&str, zbus::zvariant::Value<'_>> = HashMap::new();
    ipv4.insert("method", zbus::zvariant::Value::new("shared"));
    settings.insert("ipv4", ipv4);

    let mut ipv6: HashMap<&str, zbus::zvariant::Value<'_>> = HashMap::new();
    ipv6.insert("method", zbus::zvariant::Value::new("ignore"));
    settings.insert("ipv6", ipv6);

    let mut connection: HashMap<&str, zbus::zvariant::Value<'_>> = HashMap::new();
    // See https://developer.gnome.org/NetworkManager/stable/nm-settings.html
    connection.insert("autoconnect", zbus::zvariant::Value::new(true));
    settings.insert("connection", connection);

    settings
}

pub async fn start_ap_mode<'a>(
    dbus_connection: &'a Connection,
    network_manager: &NetworkManagerProxy<'a>,
) -> (
    OwnedObjectPath,
    (
        OwnedObjectPath,
        OwnedObjectPath,
        HashMap<String, OwnedValue>,
    ),
) {
    info!("Starting AP mode...");

    // Find wifi device
    let wifi_devices = stream::iter(
        network_manager
            .get_devices()
            .await
            .expect("Could not find devices"),
    )
    .filter_map(|device| {
        let dbus_connection = &dbus_connection;
        async move {
            match DeviceProxy::new_from_path(device.clone(), dbus_connection).await {
                Ok(device_proxy) => {
                    let device_type = rusty_network_manager::DeviceType::from_code(
                        device_proxy.device_type().await.unwrap_or_default(),
                    )
                    .unwrap_or(rusty_network_manager::DeviceType::Generic);

                    // Check if the device is a Wi-Fi device
                    if matches!(device_type, rusty_network_manager::DeviceType::WiFi) {
                        Some(device)
                    } else {
                        None
                    }
                }
                Err(e) => {
                    debug!("Failed to get device proxy: {:?}", e);
                    None
                }
            }
        }
    });

    pin_mut!(wifi_devices);

    let wifi_device = wifi_devices.next().await.expect("No wifi device found");

    network_manager
        .set_wireless_enabled(true)
        .await
        .expect("Failed to enable wireless");

    let mut options = HashMap::new();
    options.insert("persist", zbus::zvariant::Value::new("volatile"));

    let ap_connection = network_manager
        .add_and_activate_connection2(
            make_arguments_for_ap(env!("AP_NAME")),
            &wifi_device,
            &ObjectPath::try_from("/").unwrap(),
            options,
        )
        .await
        .expect("Failed to add and activate connection");

    (wifi_device, ap_connection)
}

pub async fn wait_for_ip_from_device<'a>(
    device: &DeviceProxy<'a>,
    dbus_connection: &Connection,
) -> Ipv4Addr {
    let ap_ipv4 = device.ip4_config().await.expect("Failed to get ip4 config");

    let ap_ipv4 = IP4ConfigProxy::new_from_path(ap_ipv4, dbus_connection)
        .await
        .expect("Failed to get ip4 config proxy");

    let stream = ap_ipv4.receive_addresses_changed().await;

    let ip_stream = stream.filter_map(|prop_update| async move {
        match prop_update.get().await {
            Ok(update) => {
                if let Some(ipv4_update) = update.first() {
                    if let Some(new_ip_config) = ipv4_update.first() {
                        return Some(Ipv4Addr::from(new_ip_config.to_be()));
                    }
                }
                None
            }
            Err(_) => None,
        }
    });

    pin_mut!(ip_stream);

    ip_stream.next().await.expect("Failed to get IP address")
}
