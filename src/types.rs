use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tokio::sync::mpsc;

use mlua::Lua;
use uuid::Uuid;
use wayland_client::{protocol::wl_seat, QueueHandle};
use wayland_protocols::ext::idle_notify::v1::client::{
    ext_idle_notification_v1, ext_idle_notifier_v1,
};

use crate::wayland::Output;

#[derive(Debug)]
pub enum Request {
    LuaReload,
    LuaMethod(String),
    Reset,
    OnBattery(bool),
    Flush,
    Inhibit,
}

pub type NotificationListHandle =
    Arc<Mutex<HashMap<Uuid, (String, ext_idle_notification_v1::ExtIdleNotificationV1)>>>;

pub type CallbackListHandle = Arc<Mutex<HashMap<String, String>>>;
pub type LuaHandle = Arc<Mutex<Lua>>;

#[derive(Debug)]
pub struct State {
    pub(crate) wl_seat: Option<wl_seat::WlSeat>,
    pub(crate) qh: QueueHandle<State>,
    pub(crate) idle_notifier: Option<ext_idle_notifier_v1::ExtIdleNotifierV1>,
    pub(crate) notification_list: NotificationListHandle,
    pub(crate) dbus_handlers: CallbackListHandle,
    pub(crate) tx: mpsc::Sender<Request>,
    pub(crate) lua: LuaHandle,
    pub(crate) outputs: HashMap<u32, Output>,
}
