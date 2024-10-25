use clap::Parser;
use color::Color;
use env_logger::{Builder, Env};
use inotify::{EventMask, Inotify, WatchMask};
use log::{debug, error, info};
use mlua::{AnyUserDataExt, Function, Lua, UserData, UserDataMethods};
use std::{
    collections::HashMap,
    env,
    fs::{self, File},
    io::Write,
    path::Path,
    process::Stdio,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use sysinfo::{ProcessExt, System, SystemExt};
use tokio::{process::Command, sync::mpsc, task::JoinHandle, time::sleep};
use uuid::Uuid;
use wayland_client::{
    backend::ReadEventsGuard,
    protocol::{
        wl_compositor, wl_output, wl_registry, wl_seat,
        wl_surface::{self, WlSurface},
    },
    Connection, Dispatch, EventQueue, QueueHandle,
};
use wayland_protocols::{
    ext::idle_notify::v1::client::{ext_idle_notification_v1, ext_idle_notifier_v1},
    wp::idle_inhibit::zv1::client::{
        zwp_idle_inhibit_manager_v1,
        zwp_idle_inhibitor_v1::{self, ZwpIdleInhibitorV1},
    },
    xdg::activation::v1::client::{xdg_activation_token_v1, xdg_activation_v1},
};
use wayland_protocols_wlr::gamma_control::v1::client::{
    zwlr_gamma_control_manager_v1, zwlr_gamma_control_v1,
};

use crate::types::CallbackListHandle;
use crate::types::LuaHandle;
use crate::types::NotificationListHandle;

mod color;
mod config;
mod dbus;
mod joystick_handler;
mod types;
mod udev_handler;
mod utils;

use types::Request;
use udev_handler::UdevHandler;

lazy_static::lazy_static! {
    static ref INHIBIT_MANAGER: std::sync::Mutex<Option<zwp_idle_inhibit_manager_v1::ZwpIdleInhibitManagerV1>> = std::sync::Mutex::new(None);
    static ref SURFACE: std::sync::Mutex<Option<WlSurface>> = std::sync::Mutex::new(None);
}
static IS_INHIBITED: AtomicBool = AtomicBool::new(false);

fn ensure_config_file_exists(filename: &str) -> std::io::Result<()> {
    let config_path = utils::xdg_config_path(Some(filename.to_string()))?;

    if !config_path.exists() {
        // Write the default settings to the file
        let mut file = File::create(&config_path)?;
        file.write_all(config::CONFIG_FILE.as_bytes())?;
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
    tasks: Mutex<HashMap<String, JoinHandle<anyhow::Result<()>>>>,
    //gamma_control: Option<zwlr_gamma_control_v1::ZwlrGammaControlV1>,
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
            debug!("UnlockHandler callback");
            let mut map = this.handlers.lock().unwrap();
            map.insert("UnlockHandler".to_string(), fn_name);
            Ok(())
        });
    }
}

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

        async fn run(cmd: String) -> JoinHandle<Result<(), anyhow::Error>> {
            let (cmd, args) = utils::get_args(cmd.clone());

            tokio::spawn(async move {
                match Command::new(&cmd)
                    .env(
                        "WAYLAND_DISPLAY",
                        env::var("WAYLAND_DISPLAY").unwrap_or_default(),
                    )
                    .env(
                        "DBUS_SESSION_BUS_ADDRESS",
                        env::var("DBUS_SESSION_BUS_ADDRESS").unwrap_or_default(),
                    )
                    .stdout(Stdio::inherit())
                    .stderr(Stdio::inherit())
                    .args(args)
                    .spawn()
                {
                    Ok(mut child) => match child.wait().await {
                        Ok(status) => {
                            debug!("Command {} completed with status: {:?}", cmd, status);
                            Ok(())
                        }
                        Err(e) => {
                            error!("{} process failed to run: {}", cmd, e);
                            Err(anyhow::Error::msg(format!("Failed to run command: {}", e)))
                        }
                    },
                    Err(e) => {
                        error!("Failed to spawn {} process: {}", cmd, e);
                        Err(anyhow::Error::msg(format!(
                            "Failed to spawn process: {}",
                            e
                        )))
                    }
                }
            })
        }

        methods.add_async_method("run", |_lua, _this, command: String| async move {
            debug!("run function called {}", command.clone());
            let _handle = run(command).await;
            Ok(())
        });

        methods.add_async_method("run_once", |_lua, _this, command: String| async move {
            debug!("run_once function called {}", command.clone());
            let s = System::new_all();
            let (cmd_name, _) = utils::get_args(command.clone());

            // Check if the process is already running
            let is_running = s
                .processes_by_exact_name(&cmd_name)
                .any(|p| p.name() == cmd_name);

            if !is_running {
                //let mut tasks = this.tasks.lock();
                //if !tasks.contains_key(&cmd) {
                let _handle = run(command.clone()).await;
                //tasks.insert(cmd_name, handle);
                //}
            }
            Ok(())
        });
    }
}

fn generate_uuid() -> uuid::Uuid {
    Uuid::new_v4()
}

async fn _wait_for_wayland_event(
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

    let _spawn_blocking = tokio::task::spawn_blocking(move || loop {
        let events = inotify
            .read_events_blocking(&mut buffer)
            .expect("Failed to read inotify events");

        debug!("Received events");
        for event in events {
            debug!("File modified: {:?}", event.name);
            if event.mask.contains(EventMask::MODIFY) && !event.mask.contains(EventMask::ISDIR) {
                debug!("File modified: {:?}", event.name);
                tx.blocking_send(Request::Reset).unwrap();
            }
        }
    });
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
        tasks: Mutex::new(HashMap::new()),
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
        if let wl_output::Event::Geometry {
            x,
            y,
            physical_width,
            physical_height,
            subpixel,
            make,
            model,
            transform,
        } = event
        {
            info!(
                "Output geometry: x: {}, y: {}, physical_width: {}, physical_height: {}, subpixel: {:?}, make: {}, model: {}, transform: {:?}",
                x, y, physical_width, physical_height, subpixel, make, model, transform
            );
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
                "xdg_activation_token_v1" => {
                    let _activation = registry
                        .bind::<xdg_activation_token_v1::XdgActivationTokenV1, _, _>(
                            name,
                            1,
                            qh,
                            (),
                        );
                    info!("xdg_activation_token_v1: {:?}", name);
                }
                // Idle inhibitor is used to handle sleep events for joystick input
                "zwp_idle_inhibitor_v1" => {
                    let _inhibitor = registry
                        .bind::<zwp_idle_inhibitor_v1::ZwpIdleInhibitorV1, _, _>(name, 1, qh, ());
                    info!("zwp_idle_inhibitor_v1: {:?}", name);
                }
                "zwp_idle_inhibit_manager_v1" => {
                    let inhibit_manager = registry
                        .bind::<zwp_idle_inhibit_manager_v1::ZwpIdleInhibitManagerV1, _, _>(
                        name,
                        1,
                        qh,
                        (),
                    );

                    *INHIBIT_MANAGER.lock().unwrap() = Some(inhibit_manager);

                    info!("zwp_idle_inhibit_manager_v1: {:?}", name);
                }
                "zwlr_gamma_control_v1" => {
                    let _gamma_control = registry
                        .bind::<zwlr_gamma_control_v1::ZwlrGammaControlV1, _, _>(name, 1, qh, ());
                    info!("zwlr_gamma_control_v1: {:?}", name);
                    //state.gamma_control = Some(_gamma_control);
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
                "wl_compositor" => {
                    let compositor =
                        registry.bind::<wl_compositor::WlCompositor, _, _>(name, 1, qh, ());
                    info!("wl_compositor: {:?}", name);

                    let surface = compositor.create_surface(qh, ());
                    *SURFACE.lock().unwrap() = Some(surface);
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

impl Dispatch<zwp_idle_inhibitor_v1::ZwpIdleInhibitorV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &zwp_idle_inhibitor_v1::ZwpIdleInhibitorV1,
        _event: zwp_idle_inhibitor_v1::Event,
        _: &(),
        _: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        info!("Idle inhibitor event: {:?}", _event)
    }
}

impl Dispatch<zwp_idle_inhibit_manager_v1::ZwpIdleInhibitManagerV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &zwp_idle_inhibit_manager_v1::ZwpIdleInhibitManagerV1,
        _event: zwp_idle_inhibit_manager_v1::Event,
        _: &(),
        _: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        info!("Idle Inhibit Manager event: {:?}", _event);
    }
}

impl Dispatch<zwlr_gamma_control_v1::ZwlrGammaControlV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &zwlr_gamma_control_v1::ZwlrGammaControlV1,
        _event: zwlr_gamma_control_v1::Event,
        _: &(),
        _: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        info!("Gamma Control: {:?}", _event);
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
        info!("Gamma Control: {:?} {:?}", manager, _event);
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

impl Dispatch<xdg_activation_token_v1::XdgActivationTokenV1, ()> for State {
    fn event(
        _: &mut Self,
        _: &xdg_activation_token_v1::XdgActivationTokenV1,
        _: xdg_activation_token_v1::Event,
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

impl Dispatch<wl_compositor::WlCompositor, ()> for State {
    fn event(
        _: &mut Self,
        _: &wl_compositor::WlCompositor,
        _: wl_compositor::Event,
        _: &(),
        _: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        info!("Compositor event");
    }
}

impl Dispatch<wl_surface::WlSurface, ()> for State {
    fn event(
        _: &mut Self,
        _: &wl_surface::WlSurface,
        _: wl_surface::Event,
        _: &(),
        _: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        info!("Surface event");
    }
}

#[derive(Clone)]
pub struct WaylandRunner {
    connection: Connection,
    qhandle: QueueHandle<State>,
    lua: LuaHandle,
    tx: mpsc::Sender<Request>,
    notification_list: NotificationListHandle,
    dbus_handlers: CallbackListHandle,
}

impl WaylandRunner {
    pub fn new(
        connection: Connection,
        qhandle: QueueHandle<State>,
        lua: LuaHandle,
        tx: mpsc::Sender<Request>,
    ) -> Self {
        let map: HashMap<Uuid, (String, ext_idle_notification_v1::ExtIdleNotificationV1)> =
            HashMap::new();

        let notification_list = Arc::new(Mutex::new(map));
        let dbus_handlers = Arc::new(Mutex::new(HashMap::new()));

        Self {
            connection,
            qhandle,
            lua,
            tx,
            notification_list,
            dbus_handlers,
        }
    }

    pub async fn wayland_run(
        &self,
        mut event_queue: EventQueue<State>,
    ) -> anyhow::Result<JoinHandle<Result<(), anyhow::Error>>> {
        let display = self.connection.display();
        display.get_registry(&self.qhandle, ());

        let mut state = State {
            wl_seat: None,
            idle_notifier: None,
            qh: self.qhandle.clone(),
            notification_list: self.notification_list.clone(),
            dbus_handlers: self.dbus_handlers.clone(),
            tx: self.tx.clone(),
            lua: self.lua.clone(),
            outputs: HashMap::new(),
        };

        Ok(tokio::task::spawn_blocking(move || loop {
            event_queue.blocking_dispatch(&mut state)?;
        }))
    }

    pub async fn process_command(&self, rx: &mut mpsc::Receiver<Request>) -> anyhow::Result<()> {
        while let Some(event) = rx.recv().await {
            match event {
                Request::Reset => {
                    debug!("Reloading config");
                    {
                        let map = self.notification_list.lock().unwrap();
                        for (_, (_, notification)) in map.iter() {
                            notification.destroy();
                        }
                        self.connection.flush();
                    }
                    self.tx.send(Request::LuaReload).await.unwrap();
                }
                Request::LuaReload => {
                    debug!("Reloading lua config");
                    let lua = self.lua.lock().unwrap();
                    let _ = lua_load_config(&lua).unwrap();
                }
                Request::LuaMethod(method_name) => {
                    let lua = self.lua.lock().unwrap();
                    let globals = lua.globals();
                    let map = self.dbus_handlers.lock().unwrap();
                    match map.get(&method_name) {
                        Some(fn_name) => {
                            let fn_name = fn_name.clone();
                            let result: Result<Function, _> = globals.get(fn_name.clone());
                            if let Ok(lua_func) = result {
                                lua_func.call(())?;
                            } else {
                                debug!("Lua function not found: {}", fn_name);
                            }
                        }
                        None => {
                            debug!("No dbus handler found for {}", method_name);
                        }
                    }
                }
                Request::OnBattery(state) => {
                    let lua = self.lua.lock().unwrap();
                    let globals = lua.globals();
                    let res: mlua::Result<mlua::AnyUserData> = globals.get("Helpers");

                    match res {
                        Ok(helpers) => {
                            let _ = helpers.call_method::<_, bool>("set_on_battery", state);
                        }
                        Err(_e) => {}
                    }
                }
                Request::Inhibit => {
                    let _ = self.inhibit_sleep();
                }
                Request::Flush => {
                    self.connection.flush();
                }
            }
        }
        Ok(())
    }

    fn inhibit_sleep(&self) -> anyhow::Result<()> {
        async fn run(connection: Connection, qhandle: QueueHandle<State>) -> anyhow::Result<()> {
            // Return early if already inhibited
            if IS_INHIBITED.load(Ordering::SeqCst) {
                return Ok(());
            }

            debug!("Inhibiting sleep");
            IS_INHIBITED.store(true, Ordering::SeqCst);

            let mut inhibitor: Option<ZwpIdleInhibitorV1> = None;
            if let Some(manager) = INHIBIT_MANAGER.lock().unwrap().as_ref() {
                let surface = SURFACE.lock().unwrap();
                if let Some(surface) = surface.as_ref() {
                    inhibitor = Some(manager.create_inhibitor(surface, &qhandle.clone(), ()));
                    let _ = connection.flush();
                }
            }
            sleep(Duration::from_secs(config::TIMEOUT_SEC)).await;

            if let Some(inhibitor) = inhibitor {
                debug!("Destroying inhibitor");
                inhibitor.destroy();
                let _ = connection.flush();
            }

            // Reset inhibited state
            IS_INHIBITED.store(false, Ordering::SeqCst);

            Ok(())
        }
        let qh = self.qhandle.clone();
        let connection = self.connection.clone();
        tokio::spawn(async move { run(connection, qh).await });
        Ok(())
    }
}
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    Builder::from_env(Env::default().default_filter_or("info")).init();
    let _ = ensure_config_file_exists(config::CONFIG_FILE_NAME);
    let (tx, mut rx) = mpsc::channel(32);

    let lua = Arc::new(Mutex::new(Lua::new()));

    let config_path = utils::xdg_config_path(None)?;
    filewatcher_run(&config_path, tx.clone())
        .await
        .expect("Failed to spawn task");

    let connection = Connection::connect_to_env().unwrap();
    let event_queue: EventQueue<State> = connection.new_event_queue();
    let qhandle = event_queue.handle();

    let wayland_runner = WaylandRunner::new(connection, qhandle.clone(), lua.clone(), tx.clone());
    let udev_handler = UdevHandler::new(tx.clone());

    let _ = wayland_runner.wayland_run(event_queue).await;

    tokio::try_join!(
        dbus::upower_watcher(tx.clone()),
        dbus::logind_watcher(tx.clone()),
        wayland_runner.process_command(&mut rx),
        udev_handler.monitor()
    )?;

    Ok(())
}
