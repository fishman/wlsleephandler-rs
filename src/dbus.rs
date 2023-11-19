use super::types::Request;
use futures::stream::StreamExt;
use tokio::sync::mpsc;
use zbus::dbus_proxy;

pub async fn upower_watcher(tx: mpsc::Sender<Request>) -> anyhow::Result<()> {
    // Establish a connection to the D-Bus system bus
    let conn = zbus::Connection::system().await?;
    let proxy = UPowerInterfaceProxy::new(&conn).await?;

    let mut state = proxy.on_battery().await?;
    let mut power_stream = proxy.receive_on_battery_changed().await;
    tx.send(Request::BatteryState(state)).await.unwrap();

    tokio::spawn(async move {
        while let Some(property_changed) = power_stream.next().await {
            state = proxy.on_battery().await.unwrap();
            tx.send(Request::BatteryState(state)).await.unwrap();
        }
    });
    Ok(())
}

#[dbus_proxy(
    interface = "org.freedesktop.UPower",
    default_service = "org.freedesktop.UPower",
    default_path = "/org/freedesktop/UPower"
)]
trait UPowerInterface {
    #[dbus_proxy(property)]
    fn on_battery(&self) -> zbus::Result<bool>;
}
