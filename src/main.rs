use std::net::{SocketAddr, SocketAddrV4};

use ap::settings::{start_ap_mode, wait_for_ip_from_device};
use futures::stream::{self, StreamExt};
use portal::{ip_tables::configure_iptables, router::start_portal};
use rusty_network_manager::{DeviceProxy, NetworkManagerProxy};
use tracing::{debug, info};
use zbus::Connection;

mod ap;
mod portal;

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

    if !network_connected || option_env!("SKIP_NETWORK_TEST").unwrap_or("false") == "true" {
        let ap_values = start_ap_mode(&dbus_connection, &network_manager).await;

        let ap_device = DeviceProxy::new_from_path(ap_values.0, &dbus_connection)
            .await
            .expect("Failed to get ap device");

        let ap_interface_name = ap_device
            .interface()
            .await
            .expect("Failed to get ap device interface name");

        info!("AP Interface name: {}", ap_interface_name);

        let ap_ip = wait_for_ip_from_device(&ap_device, &dbus_connection).await;
        info!("AP IP: {}", ap_ip);

        configure_iptables(&ap_interface_name, format!("{}:3000", ap_ip).as_str());

        let socket_portal_address = SocketAddr::V4(SocketAddrV4::new(ap_ip, 3000));
        start_portal(&socket_portal_address).await;

        let _ = network_manager
            .deactivate_connection(&ap_values.1 .1.as_ref())
            .await;
    } else {
        info!("Already connected to network, exiting...")
    }
}
