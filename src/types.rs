use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tokio::{process::Command, sync::mpsc, task::JoinHandle, time::sleep};

use mlua::Lua;
use uuid::Uuid;
use wayland_client::{
    protocol::{wl_output, wl_seat},
    QueueHandle,
};
use wayland_protocols::ext::idle_notify::v1::client::{
    ext_idle_notification_v1, ext_idle_notifier_v1,
};

use crate::{color::Color, wayland::Output};

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
    pub wl_seat: Option<wl_seat::WlSeat>,
    pub qh: QueueHandle<State>,
    pub idle_notifier: Option<ext_idle_notifier_v1::ExtIdleNotifierV1>,
    pub notification_list: NotificationListHandle,
    pub dbus_handlers: CallbackListHandle,
    pub tx: mpsc::Sender<Request>,
    pub lua: LuaHandle,
    pub outputs: HashMap<u32, Output>,
}
