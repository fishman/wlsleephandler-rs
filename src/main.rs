use clap::Parser;
use color::{colorramp_fill, Color};
use env_logger::{Builder, Env};
use inotify::{EventMask, Inotify, WatchMask};
use log::{debug, error, info};
use mlua::{AnyUserDataExt, Function, Lua, UserData, UserDataMethods};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::{
    path::Path,
    sync::{Arc, Mutex},
};
use tokio::sync::mpsc;
use uuid::Uuid;
use wayland_client::backend::ReadEventsGuard;
use wayland_client::protocol::{wl_output, wl_registry, wl_seat};
use wayland_client::{Connection, Dispatch, EventQueue, QueueHandle};
use wayland_protocols::{
    ext::idle_notify::v1::client::{ext_idle_notification_v1, ext_idle_notifier_v1},
    xdg::activation::v1::client::xdg_activation_v1,
};
use wayland_protocols_wlr::gamma_control::v1::client::{
    zwlr_gamma_control_manager_v1, zwlr_gamma_control_v1,
};
//use nix::{
//    poll::{PollFd, PollFlags},
//    sys::timerfd::{ClockId, TimerFd, TimerFlags},
//    unistd::{close, read},
//};

const JOYSTICKS_MAX: usize = 5;
const JOYSTICKS_FD_START: usize = 3;
const FDS_MAX: usize = JOYSTICKS_FD_START + JOYSTICKS_MAX;

mod color;
mod config;
mod dbus;
mod types;
mod utils;
//mod wljoywake;

use types::Request;

const CONFIG_FILE: &str = include_str!("../lua_configs/idle_config.lua");

fn ensure_config_file_exists(filename: &str) -> std::io::Result<()> {
    let config_path = utils::xdg_config_path(Some(filename.to_string()))?;

    if !config_path.exists() {
        // Write the default settings to the file
        let mut file = File::create(&config_path)?;
        file.write_all(CONFIG_FILE.as_bytes())?;
    }

    Ok(())
}

#[derive(Debug)]
pub enum FileRequest {
    Write,
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value = config::CONFIG_FILE_NAME)]
    config: String,
}

#[derive(Debug)]
struct State {
    wl_seat: Option<wl_seat::WlSeat>,
    qh: QueueHandle<State>,
    idle_notifier: Option<ext_idle_notifier_v1::ExtIdleNotifierV1>,
    notification_list: NotificationListHandle,
    dbus_handlers: CallbackListHandle,
    tx: mpsc::Sender<Request>,
    lua: LuaHandle,
    outputs: HashMap<u32, Output>,
}

#[derive(Clone, Debug)]
struct NotificationContext {
    uuid: Uuid,
}

struct MyLuaFunctions {
    wl_seat: Option<wl_seat::WlSeat>,
    qh: QueueHandle<State>,
    idle_notifier: Option<ext_idle_notifier_v1::ExtIdleNotifierV1>,
    tx: mpsc::Sender<Request>,
    notification_list: NotificationListHandle,
}

#[derive(Clone, Debug)]
struct LuaHelpers {
    on_battery: bool,
}

#[derive(Clone, Debug)]
struct DbusHandler {
    handlers: CallbackListHandle,
}

#[derive(Debug)]
pub struct Output {
    reg_name: u32,
    wl_output: wl_output::WlOutput,
    name: Option<String>,
    color: Color,
    //gamma_control: ZwlrGammaControlV1,
    ramp_size: usize,
    color_changed: bool,
}

impl UserData for DbusHandler {
    fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_method("PrepareSleep", |_lua, this, fn_name: String| {
            debug!("PrepareSleep callback");
            let mut map = this.handlers.lock().unwrap();
            map.insert("PrepareSleep".to_string(), fn_name);
            Ok(())
        });
        methods.add_method("LockHandler", |_lua, this, fn_name: String| {
            debug!("LcokHandler callback");
            let mut map = this.handlers.lock().unwrap();
            map.insert("LockHandler".to_string(), fn_name);
            Ok(())
        });
        methods.add_method("UnlockHandler", |_lua, this, fn_name: String| {
            debug!("UnlcokHandler callback");
            let mut map = this.handlers.lock().unwrap();
            map.insert("UnlockHandler".to_string(), fn_name);
            Ok(())
        });
    }
}

type NotificationListHandle =
    Arc<Mutex<HashMap<Uuid, (String, ext_idle_notification_v1::ExtIdleNotificationV1)>>>;

type CallbackListHandle = Arc<Mutex<HashMap<String, String>>>;
type LuaHandle = Arc<Mutex<Lua>>;

impl UserData for LuaHelpers {
    // fn add_fields<'lua, F: UserDataFields<'lua, Self>>(fields: &mut F) {
    //     fields.add_field_method_get("on_battery", |_, this| Ok(this.on_battery));
    // }

    fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_method("on_battery", |_lua, this, (): ()| Ok(this.on_battery));
        methods.add_method_mut("set_on_battery", |_lua, this, value: bool| {
            this.on_battery = value;
            Ok(())
        });
        methods.add_method("log", |_lua, _this, message: String| {
            info!("{}", message);
            Ok(())
        });
    }
}

impl UserData for MyLuaFunctions {
    fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_method(
            "get_notification",
            |_lua, this, (timeout, fn_name): (i32, String)| {
                let ctx = NotificationContext {
                    uuid: generate_uuid(),
                };

                debug!(
                    "get_notification id: {} fn: {} timeout: {} seconds",
                    ctx.uuid, fn_name, timeout
                );
                let notification = this.idle_notifier.as_ref().unwrap().get_idle_notification(
                    (timeout * 1000).try_into().unwrap(),
                    this.wl_seat.as_ref().unwrap(),
                    &this.qh,
                    ctx.clone(),
                );

                {
                    let mut map = this.notification_list.lock().unwrap();
                    map.insert(ctx.uuid, (fn_name, notification));
                }

                Ok(())
            },
        );
        methods.add_method("run", |_lua, this, command: String| {
            let tx = this.tx.clone();
            std::thread::spawn(move || {
                tx.blocking_send(Request::Run(command.to_string())).unwrap();
            });
            Ok(())
        });
        methods.add_method("run_once", |_lua, this, command: String| {
            let tx = this.tx.clone();
            std::thread::spawn(move || {
                tx.blocking_send(Request::RunOnce(command.to_string()))
                    .unwrap();
            });
            Ok(())
        });
    }
}

fn generate_uuid() -> uuid::Uuid {
    Uuid::new_v4()
}

pub async fn wayland_run(
    lua: LuaHandle,
    tx: mpsc::Sender<Request>,
    notification_list: NotificationListHandle,
    dbus_handlers: CallbackListHandle,
) -> anyhow::Result<(), anyhow::Error> {
    let conn = Connection::connect_to_env().unwrap();
    let mut event_queue: EventQueue<State> = conn.new_event_queue();
    let qhandle = event_queue.handle();

    let display = conn.display();
    display.get_registry(&qhandle, ());

    let mut state = State {
        wl_seat: None,
        idle_notifier: None,
        qh: qhandle.clone(),
        notification_list: notification_list.clone(),
        dbus_handlers: dbus_handlers.clone(),
        tx: tx.clone(),
        lua,
        outputs: HashMap::new(),
    };

    let _ = tokio::task::spawn_blocking(move || loop {
        event_queue.blocking_dispatch(&mut state).unwrap();
    });
    Ok(())
}

async fn wait_for_wayland_event(
    read_guard: ReadEventsGuard,
    event_queue: &mut EventQueue<State>,
    state: &mut State,
) {
    // to turn synchronous Wayland socket events into async events
    // If epoll notified readiness of the Wayland socket, you can now proceed to the read
    read_guard.read().unwrap();
    // And now, you must invoke dispatch_pending() to actually process the events
    event_queue.dispatch_pending(state).unwrap();
}

pub async fn filewatcher_run(config_path: &Path, tx: mpsc::Sender<Request>) -> anyhow::Result<()> {
    let mut inotify = Inotify::init().expect("Error while initializing inotify instance");

    debug!("Watching {:?}", config_path);
    // Watch for modify and close events.
    inotify
        .watches()
        .add(config_path, WatchMask::MODIFY)
        .expect("Failed to add file watch");

    let mut buffer = [0; 1024];

    let _ = tokio::task::spawn_blocking(move || loop {
        let events = inotify
            .read_events_blocking(&mut buffer)
            .expect("Failed to read inotify events");

        debug!("Received events");
        for event in events {
            debug!("File modified: {:?}", event.name);
            if event.mask.contains(EventMask::MODIFY) {
                if !event.mask.contains(EventMask::ISDIR) {
                    debug!("File modified: {:?}", event.name);
                    tx.blocking_send(Request::Reset).unwrap();
                }
            }
        }
    });
    Ok(())
}

async fn process_command(
    lua: LuaHandle,
    tx: mpsc::Sender<Request>,
    rx: &mut mpsc::Receiver<Request>,
    shared_map: NotificationListHandle,
    dbus_handlers: CallbackListHandle,
) -> anyhow::Result<()> {
    while let Some(event) = rx.recv().await {
        match event {
            Request::Reset => {
                debug!("Reloading config");
                {
                    let map = shared_map.lock().unwrap();
                    for (_, (_, notification)) in map.iter() {
                        notification.destroy();
                    }
                }
                tx.send(Request::LuaReload).await.unwrap();
            }
            Request::LuaReload => {
                debug!("Reloading lua config");
                let lua = lua.lock().unwrap();
                let _ = lua_load_config(&lua).unwrap();
            }
            Request::LuaMethod(method_name) => {
                let lua = lua.lock().unwrap();
                let globals = lua.globals();
                let map = dbus_handlers.lock().unwrap();
                match map.get(&method_name) {
                    Some(fn_name) => {
                        let fn_name = fn_name.clone();
                        let result: Result<Function, _> = globals.get(fn_name.clone());
                        if let Ok(lua_func) = result {
                            let lua_result: i32 =
                                lua_func.call(()).map_err(|e| anyhow::Error::new(e))?;
                            let _result: Result<i32, anyhow::Error> = Ok(lua_result);
                        } else {
                            debug!("Lua function not found: {}", fn_name);
                        }
                    }
                    None => {
                        debug!("No dbus handler found for {}", method_name);
                    }
                }
            }
            Request::Run(cmd) => {
                debug!("Running command: {}", cmd);
                let _ = utils::run(cmd).await;
            }
            Request::RunOnce(cmd) => {
                debug!("Running command once: {}", cmd);
                let _ = utils::run_once(cmd).await;
            }
            Request::OnBattery(state) => {
                let lua = lua.lock().unwrap();
                let globals = lua.globals();
                let res: mlua::Result<mlua::AnyUserData> = globals.get("Helpers");

                match res {
                    Ok(helpers) => {
                        let _ = helpers.call_method::<_, bool>("set_on_battery", state);
                    }
                    Err(_e) => {}
                }
            }
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    Builder::from_env(Env::default().default_filter_or("info")).init();
    let _ = ensure_config_file_exists(config::CONFIG_FILE_NAME);
    // Run the event loop in a separate async task
    let (tx, mut rx) = mpsc::channel(32);

    let map: HashMap<Uuid, (String, ext_idle_notification_v1::ExtIdleNotificationV1)> =
        HashMap::new();
    let shared_map = Arc::new(Mutex::new(map));
    let lua = Arc::new(Mutex::new(Lua::new()));
    let dbus_handlers = Arc::new(Mutex::new(HashMap::new()));

    let config_path = utils::xdg_config_path(None)?;
    let _task = filewatcher_run(&config_path, tx.clone())
        .await
        .expect("Failed to spawn task");
    let _ = wayland_run(
        lua.clone(),
        tx.clone(),
        shared_map.clone(),
        dbus_handlers.clone(),
    )
    .await;
    tokio::try_join!(
        dbus::upower_watcher(tx.clone()),
        dbus::logind_watcher(tx.clone()),
        process_command(
            lua.clone(),
            tx,
            &mut rx,
            shared_map.clone(),
            dbus_handlers.clone()
        ),
    )?;
    // .await
    // .unwrap();

    Ok(())
}

fn lua_load_config(lua: &Lua) -> anyhow::Result<Result<(), mlua::Error>> {
    let args = Args::parse();

    let config_path = utils::xdg_config_path(Some(args.config))?;
    let config = fs::read_to_string(config_path)?;
    let result = lua.load(&config).exec();
    match result {
        Ok(_) => {}
        Err(ref e) => {
            error!("Error loading config: {}", e);
        }
    }

    Ok(result)
}

fn lua_init(state: &mut State) -> anyhow::Result<()> {
    let lua = state.lua.lock().unwrap();
    lua.sandbox(true)?;
    let my_lua_functions = MyLuaFunctions {
        wl_seat: state.wl_seat.clone(),
        idle_notifier: state.idle_notifier.clone(),
        qh: state.qh.clone(),
        notification_list: state.notification_list.clone(),
        tx: state.tx.clone(),
    };

    let globals = lua.globals();
    globals.set("IdleNotifier", my_lua_functions)?;
    globals.set("Helpers", LuaHelpers { on_battery: true })?;
    let _ = globals.set(
        "DbusHandler",
        DbusHandler {
            handlers: state.dbus_handlers.clone(),
        },
    );
    let _ = lua_load_config(&lua)?;

    Ok(())
}

impl Dispatch<wl_output::WlOutput, ()> for State {
    fn event(
        _state: &mut Self,
        _output: &wl_output::WlOutput,
        event: wl_output::Event,
        _: &(),
        _: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_output::Event::Geometry {
                x,
                y,
                physical_width,
                physical_height,
                subpixel,
                make,
                model,
                transform,
            } => {
                info!(
                    "Output geometry: x: {}, y: {}, physical_width: {}, physical_height: {}, subpixel: {:?}, make: {}, model: {}, transform: {:?}",
                    x, y, physical_width, physical_height, subpixel, make, model, transform
                );
            }
            //wl_output::Event::Mode {
            //    flags: _,
            //    width,
            //    height,
            //    refresh,
            //} => {
            //    info!(
            //        "Output mode: width: {}, height: {}, refresh: {}",
            //        width, height, refresh
            //    );
            //}
            _ => {}
        }
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name, interface, ..
        } = event
        {
            match &interface[..] {
                "wl_seat" => {
                    let wl_seat = registry.bind::<wl_seat::WlSeat, _, _>(name, 1, qh, ());
                    state.wl_seat = Some(wl_seat.clone());
                    debug!("wl_seat: {:?}", name);
                    if state.wl_seat.is_some() && state.idle_notifier.is_some() {
                        let _ = lua_init(state);
                    }
                }
                "ext_idle_notifier_v1" => {
                    let idle_notifier = registry
                        .bind::<ext_idle_notifier_v1::ExtIdleNotifierV1, _, _>(name, 1, qh, ());

                    debug!("ext_idle_notifier_v1: {:?}", name);
                    state.idle_notifier = Some(idle_notifier);
                    if state.wl_seat.is_some() && state.idle_notifier.is_some() {
                        let _ = lua_init(state);
                    }
                }
                "xdg_activation_v1" => {
                    let _activation =
                        registry.bind::<xdg_activation_v1::XdgActivationV1, _, _>(name, 1, qh, ());
                    info!("xdg_activation_v1: {:?}", name);
                }
                "zwlr_gamma_control_v1" => {
                    let _gamma_control = registry
                        .bind::<zwlr_gamma_control_v1::ZwlrGammaControlV1, _, _>(name, 1, qh, ());
                    info!("zwlr_gamma_control_v1: {:?}", name);
                }
                "zwlr_gamma_control_manager_v1" => {
                    let _gamma_control_manager =
                        registry
                            .bind::<zwlr_gamma_control_manager_v1::ZwlrGammaControlManagerV1, _, _>(
                                name,
                                1,
                                qh,
                                (),
                            );
                    info!("zwlr_gamma_control_manager_v1: {:?}", name);
                }
                "wl_output" => {
                    let wl_output = registry.bind::<wl_output::WlOutput, _, _>(name, 1, qh, ());
                    let output = Output {
                        reg_name: name,
                        wl_output,
                        name: None,
                        color: Color::default(),
                        ramp_size: 0,
                        color_changed: false,
                    };
                    state.outputs.insert(name, output);
                    info!("wl_output: {:?}", name);
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for State {
    fn event(
        _: &mut Self,
        _: &wl_seat::WlSeat,
        _: wl_seat::Event,
        _: &(),
        _: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<zwlr_gamma_control_v1::ZwlrGammaControlV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &zwlr_gamma_control_v1::ZwlrGammaControlV1,
        _: zwlr_gamma_control_v1::Event,
        _: &(),
        _: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<zwlr_gamma_control_manager_v1::ZwlrGammaControlManagerV1, ()> for State {
    fn event(
        _: &mut Self,
        manager: &zwlr_gamma_control_manager_v1::ZwlrGammaControlManagerV1,
        _event: zwlr_gamma_control_manager_v1::Event,
        _: &(),
        _: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        info!("Gamma Control: {:?}", manager);
    }
}

impl Dispatch<xdg_activation_v1::XdgActivationV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &xdg_activation_v1::XdgActivationV1,
        _: xdg_activation_v1::Event,
        _: &(),
        _: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        info!("XdgActivation event");
    }
}

impl Dispatch<ext_idle_notifier_v1::ExtIdleNotifierV1, ()> for State {
    fn event(
        _state: &mut Self,
        _idle_notifier: &ext_idle_notifier_v1::ExtIdleNotifierV1,
        _event: ext_idle_notifier_v1::Event,
        _: &(),
        _: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ext_idle_notification_v1::ExtIdleNotificationV1, NotificationContext> for State {
    fn event(
        state: &mut Self,
        _idle_notification: &ext_idle_notification_v1::ExtIdleNotificationV1,
        event: ext_idle_notification_v1::Event,
        ctx: &NotificationContext,
        _: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        debug!("Idle Notification: {:?} {:?}", event, ctx.uuid);
        let map = state.notification_list.lock().unwrap();
        let binding = state.lua.lock().unwrap();
        let globals = binding.globals();
        let fn_name = map.get(&ctx.uuid).unwrap().0.clone();
        let handler: Function = globals.get(fn_name).unwrap();
        let _ = handler.call::<_, ()>(match event {
            ext_idle_notification_v1::Event::Idled => "idled",
            ext_idle_notification_v1::Event::Resumed => "resumed",
            _ => "unknown",
        });
    }
}
