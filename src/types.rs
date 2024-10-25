use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use mlua::Lua;
use uuid::Uuid;
use wayland_protocols::ext::idle_notify::v1::client::ext_idle_notification_v1;

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
