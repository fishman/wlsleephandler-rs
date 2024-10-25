use super::types::Request;
use futures::stream::StreamExt;
use log::{debug, error};
use tokio::sync::mpsc;
use zbus::dbus_proxy;

pub async fn upower_watcher(tx: mpsc::Sender<Request>) -> anyhow::Result<()> {
    let conn = zbus::Connection::system().await?;
    let proxy = UPowerInterfaceProxy::new(&conn).await?;

    let state = proxy.on_battery().await?;
    let mut power_stream = proxy.receive_on_battery_changed().await;
    tx.send(Request::OnBattery(state)).await.unwrap();

    tokio::spawn(async move {
        while let Some(on_battery_changed) = power_stream.next().await {
            match on_battery_changed.get().await {
                Ok(on_battery) => {
                    tx.send(Request::OnBattery(on_battery)).await.unwrap();
                }
                Err(e) => {
                    error!("Error, getting on_battery property {}", e)
                }
            }
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

#[dbus_proxy(
    interface = "org.freedesktop.login1.Manager",
    default_service = "org.freedesktop.login1",
    default_path = "/org/freedesktop/login1"
)]
trait LogindManagerInterface {
    #[dbus_proxy(signal)]
    fn prepare_for_sleep(&self, start: bool) -> fdo::Result<()>;
}

#[dbus_proxy(
    interface = "org.freedesktop.login1.Session",
    default_service = "org.freedesktop.login1",
    default_path = "/org/freedesktop/login1"
)]
trait LogindSessionInterface {
    #[dbus_proxy(signal)]
    fn lock(&self) -> fdo::Result<()>;
    #[dbus_proxy(signal)]
    fn unlock(&self) -> fdo::Result<()>;
}

pub async fn logind_watcher(tx: mpsc::Sender<Request>) -> anyhow::Result<()> {
    let conn = zbus::Connection::system().await?;
    let session_proxy = LogindSessionInterfaceProxy::new(&conn).await?;
    let manager_proxy = LogindManagerInterfaceProxy::new(&conn).await?;

    tokio::spawn(async move {
        let mut lock_stream = session_proxy.receive_lock().await.unwrap();
        let mut unlock_stream = session_proxy.receive_unlock().await.unwrap();
        let mut prepare_sleep_stream = manager_proxy.receive_prepare_for_sleep().await.unwrap();

        loop {
            tokio::select! {
                Some(_) = lock_stream.next() => {
                    debug!("Lock signal received");
                    let _ = tx.send(Request::LuaMethod("Lock".to_string())).await;
                },
                Some(_) = unlock_stream.next() => {
                    debug!("Unlock signal received");
                    let _ = tx.send(Request::LuaMethod("Unlock".to_string())).await;
                },
                Some(signal) = prepare_sleep_stream.next() => {
                    debug!("Prepare for Sleep signal received");
                    match signal.args() {
                        Ok(args) => {
                            if *args.start() {
                                let _ = tx.send(Request::LuaMethod("PrepareSleep".to_string())).await;
                            } else {
                                let _ = tx.send(Request::LuaMethod("Wakeup".to_string())).await;
                            }
                        }
                        Err(e) => {
                            error!("Error getting prepare_for_sleep args: {}", e);
                        }
                    }
                },
            }
        }
    });
    Ok(())
}
