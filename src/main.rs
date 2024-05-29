use std::{collections::HashMap, net::Ipv4Addr, time::Duration};

use futures::{
    pin_mut,
    stream::{self, StreamExt},
};
use rusty_network_manager::{
    AccessPointProxy, ConnectionProxy, DeviceProxy, IP4ConfigProxy, NetworkManagerProxy,
};
use tokio::time::sleep;
use tracing::{debug, info};
use zbus::{
    zvariant::{ObjectPath, OwnedObjectPath, OwnedValue},
    Connection,
};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let dbus_connection = Connection::system()
        .await
        .expect("Failed to connect to D-Bus system bus");

    // Connect to NetworkManager
    let network_manager = NetworkManagerProxy::new(&dbus_connection)
        .await
        .expect("Could not connect to NetworkManager");

    let devices = network_manager
        .get_devices()
        .await
        .expect("Could not find devices");

    // Check if connected to Ethernet
    let network_connected = stream::iter(devices)
        .any(|device| {
            let dbus_connection = &dbus_connection;
            async move {
                let device =
                    match DeviceProxy::new_from_path(device.to_owned(), dbus_connection).await {
                        Ok(device) => device,
                        Err(e) => {
                            debug!("Failed to get device proxy: {:?}", e);
                            return false;
                        }
                    };

                let state_id = match device.state().await {
                    Ok(state_id) => state_id,
                    Err(e) => {
                        debug!("Failed to get device state: {:?}", e);
                        return false;
                    }
                };

                let state = rusty_network_manager::State::from_code(state_id)
                    .unwrap_or(rusty_network_manager::State::Unknown);

                matches!(state, rusty_network_manager::State::Activated)
            }
        })
        .await;

    if
    /* !network_connected */
    true {
        let ap_values = start_ap_mode(&dbus_connection, &network_manager).await;

        let ap_device = DeviceProxy::new_from_path(ap_values.0, &dbus_connection)
            .await
            .expect("Failed to get ap device");

        let ap_ipv4 = ap_device
            .ip4_config()
            .await
            .expect("Failed to get ip4 config");

        let ap_ipv4 = IP4ConfigProxy::new_from_path(ap_ipv4, &dbus_connection)
            .await
            .expect("Failed to get ip4 config proxy");

        sleep(Duration::from_secs(1)).await;

        let stream = ap_ipv4.receive_addresses_changed().await;

        let ap_ip = stream
            .take(1)
            .map(|_| async {
                let ap_addresses = ap_ipv4
                    .addresses()
                    .await
                    .expect("Failed to get ap adresses");

                let ap_adress = ap_addresses.iter().next().expect("No ap address found");

                let ap_dec_ip = ap_adress.iter().next().unwrap();

                let ap_ip = Ipv4Addr::from(ap_dec_ip.to_be());

                ap_ip
            })
            .next()
            .await
            .expect("Failed to get ap ip")
            .await;

        info!("AP IP: {}", ap_ip);

        sleep(Duration::from_secs(3)).await;

        let _ = network_manager
            .deactivate_connection(&ap_values.1 .1.as_ref())
            .await;
    } else {
        info!("Already connected to network, exiting...")
    }
}

fn make_arguments_for_ap(ssid: &str) -> HashMap<&str, HashMap<&str, zbus::zvariant::Value<'_>>> {
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

async fn start_ap_mode<'a>(
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
            make_arguments_for_ap("test_ap"),
            &wifi_device,
            &ObjectPath::try_from("/").unwrap(),
            options,
        )
        .await
        .expect("Failed to add and activate connection");

    (wifi_device, ap_connection)
}
