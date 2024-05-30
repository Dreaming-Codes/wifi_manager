use std::{time::Duration};

use ap::settings::{start_ap_mode, wait_for_ip_from_device};
use futures::{
    stream::{self, StreamExt},
};
use rusty_network_manager::{
    DeviceProxy, NetworkManagerProxy,
};
use tokio::time::sleep;
use tracing::{debug, info};
use zbus::{
    Connection,
};

mod ap;

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

    if !network_connected || env!("SKIP_NETWORK_TEST") == "true" {
        let ap_values = start_ap_mode(&dbus_connection, &network_manager).await;

        let ap_device = DeviceProxy::new_from_path(ap_values.0, &dbus_connection)
            .await
            .expect("Failed to get ap device");

        let ap_ip = wait_for_ip_from_device(&ap_device, &dbus_connection).await;
        info!("AP IP: {}", ap_ip);

        sleep(Duration::from_secs(3)).await;

        let _ = network_manager
            .deactivate_connection(&ap_values.1 .1.as_ref())
            .await;
    } else {
        info!("Already connected to network, exiting...")
    }
}
