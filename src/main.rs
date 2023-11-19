use clap::Parser;
use inotify::{EventMask, Inotify, WatchMask};
use log::debug;
use mlua::{Function, Lua, UserData, UserDataMethods};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use uuid::Uuid;
use wayland_client::protocol::{wl_registry, wl_seat};
use wayland_client::{Connection, Dispatch, EventQueue, QueueHandle};
use wayland_protocols::ext::idle_notify::v1::client::{
    ext_idle_notification_v1, ext_idle_notifier_v1,
};

mod config;
mod utils;

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
pub enum Request {
    Reload,
    RunOnce(String),
    InitLua(wl_seat::WlSeat, ext_idle_notifier_v1::ExtIdleNotifierV1),
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
    tx: mpsc::Sender<Request>,
    lua: Lua,
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

type NotificationListHandle =
    Arc<Mutex<HashMap<Uuid, (String, ext_idle_notification_v1::ExtIdleNotificationV1)>>>;

impl UserData for MyLuaFunctions {
    fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_method(
            "get_notification",
            |_lua, this, (timeout, fn_name): (i32, String)| {
                let ctx = NotificationContext {
                    uuid: generate_uuid(),
                };

                let notification = this.idle_notifier.as_ref().unwrap().get_idle_notification(
                    (timeout * 1000).try_into().unwrap(),
                    this.wl_seat.as_ref().unwrap(),
                    &this.qh,
                    ctx.clone(),
                );

                let mut map = this.notification_list.lock().unwrap();
                map.insert(ctx.uuid, (fn_name, notification));

                Ok(())
            },
        );
        methods.add_method("run_once", |_lua, this, command: String| {
            debug!("Running command: {}", command);
            this.tx
                .blocking_send(Request::RunOnce(command.to_string()))
                .unwrap();
            Ok(())
        });
    }
}

fn generate_uuid() -> uuid::Uuid {
    Uuid::new_v4()
}

pub async fn wayland_run(
    tx: &mut mpsc::Sender<Request>,
    shared_map: NotificationListHandle,
) -> anyhow::Result<()> {
    let conn = Connection::connect_to_env().unwrap();
    let mut event_queue: EventQueue<State> = conn.new_event_queue();
    let qhandle = event_queue.handle();

    let display = conn.display();
    display.get_registry(&qhandle, ());

    let mut state = State {
        wl_seat: None,
        idle_notifier: None,
        qh: qhandle.clone(),
        notification_list: shared_map.clone(),
        tx: tx.clone(),
        lua: Lua::new(),
    };

    tokio::task::spawn_blocking(move || loop {
        event_queue.blocking_dispatch(&mut state).unwrap();
    });
    Ok(())
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

        for event in events {
            if event.mask.contains(EventMask::MODIFY) {
                if !event.mask.contains(EventMask::ISDIR) {
                    debug!("File modified: {:?}", event.name);
                    tx.blocking_send(Request::Reload).unwrap();
                }
            }
        }
    });
    Ok(())
}

async fn process_command(
    // qh: QueueHandle<State>,
    rx: &mut mpsc::Receiver<Request>,
    shared_map: NotificationListHandle,
) {
    // let mut state = State {
    //     qh: qh.clone(),
    //     notification_list: shared_map.clone(),
    //     tx: mpsc::channel(32).0,
    // };
    while let Some(event) = rx.recv().await {
        match event {
            Request::Reload => {
                debug!("Reloading config");
                let map = shared_map.lock().unwrap();
                for (_, (_, notification)) in map.iter() {
                    notification.destroy();
                }
            }
            Request::RunOnce(cmd) => {
                debug!("Running command: {}", cmd);
                let _ = utils::run_once(cmd).await;
            }
            Request::LuaFunction(fn_name, event) => {
                debug!("Running lua function");
                let func: Function = state.lua.globals().get(fn_name).unwrap();
                let _ = func.call::<_, ()>(match event {
                    ext_idle_notification_v1::Event::Idled => "idled",
                    ext_idle_notification_v1::Event::Resumed => "resumed",
                    _ => "unknown",
                });
            }
            Request::CreateNotification(timeout, fn_name) => {
                let ctx = NotificationContext {
                    uuid: generate_uuid(),
                };

                debug!("Creating notification: {:?}", ctx.uuid);
                let notification = state.idle_notifier.as_ref().unwrap().get_idle_notification(
                    (timeout * 1000).try_into().unwrap(),
                    state.wl_seat.as_ref().unwrap(),
                    state.qh.as_ref().unwrap(),
                    ctx.clone(),
                );

                let mut map = state.notification_list.lock().unwrap();
                map.insert(ctx.uuid, (fn_name, notification));
                debug!("Notification created: {:?}", ctx.uuid);
            }
            Request::LuaInit(wl_seat, idle_notifier) => {
                debug!("Initializing Lua");
                state.wl_seat = Some(wl_seat);
                state.idle_notifier = Some(idle_notifier);
                let mut lua_state = MyLuaFunctions {
                    tx: state.tx.clone(),
                    wl_seat: state.wl_seat.clone(),
                    qh: state.qh,
                    idle_notifier: state.idle_notifier.clone(),
                    notification_list: shared_map.clone(),
                };
                lua_init(&mut lua_state).unwrap();
                // state.lua = lua_init(state.tx.clone());
                // let _ = lua_init(&mut State {
                //     wl_seat: Some(wl_seat),
                //     qh: QueueHandle::dummy(),
                //     idle_notifier: Some(idle_notifier),
                //     notification_list: shared_map.clone(),
                //     tx: mpsc::channel(32).0,
                //     lua: Lua::new(),
                // });
            }
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    let _ = ensure_config_file_exists(config::CONFIG_FILE_NAME);
    // Run the event loop in a separate async task
    let (tx, mut rx) = mpsc::channel(32);

    let map: HashMap<Uuid, (String, ext_idle_notification_v1::ExtIdleNotificationV1)> =
        HashMap::new();
    let shared_map = Arc::new(Mutex::new(map));

    listen_for_ac().await?;
    let config_path = utils::xdg_config_path(None)?;
    let _task = filewatcher_run(&config_path, tx.clone())
        .await
        .expect("Failed to spawn task");
    let _ = wayland_run(&mut tx.clone(), shared_map.clone()).await;
    tokio::task::spawn(async move {
        process_command(&mut rx, shared_map.clone()).await;
    })
    .await
    .unwrap();

    Ok(())
}

fn lua_init(state: &mut State) -> anyhow::Result<()> {
    let args = Args::parse();

    let lua = &state.lua;
    lua.sandbox(true)?;
    let my_lua_functions = MyLuaFunctions {
        wl_seat: state.wl_seat.clone(),
        idle_notifier: state.idle_notifier.clone(),
        qh: state.qh.clone(),
        notification_list: state.notification_list.clone(),
        tx: state.tx.clone(),
    };

    let globals = state.lua.globals();
    globals.set("IdleNotifier", my_lua_functions)?;

    let config_path = utils::xdg_config_path(Some(args.config))?;
    let config_script = fs::read_to_string(config_path)?;

    let _result = lua.load(&config_script).exec()?;

    Ok(())
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
                    let _ = state.tx.blocking_send(Request::InitLua(
                        wl_seat.clone(),
                        state.idle_notifier.as_ref().unwrap().clone(),
                    ));
                    let _ = lua_init(state);
                }
                "ext_idle_notifier_v1" => {
                    let idle_notifier = registry
                        .bind::<ext_idle_notifier_v1::ExtIdleNotifierV1, _, _>(name, 1, qh, ());

                    debug!("ext_idle_notifier_v1: {:?}", name);
                    state.idle_notifier = Some(idle_notifier);
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
        let globals = state.lua.globals();
        let fn_name = map.get(&ctx.uuid).unwrap().0.clone();
        let tostring: Function = globals.get(fn_name).unwrap();
        let _ = tostring.call::<_, ()>(match event {
            ext_idle_notification_v1::Event::Idled => "idled",
            ext_idle_notification_v1::Event::Resumed => "resumed",
            _ => "unknown",
        });
    }
}

async fn listen_for_ac() -> anyhow::Result<()> {
    // Establish a connection to the D-Bus system bus
    let connection = zbus::Connection::system().await?;
    let proxy = zbus::Proxy::new(
        &connection,
        "org.freedesktop.UPower",
        "/org/freedesktop/UPower/devices/line_power_AC",
        "org.freedesktop.UPower.Device",
    )
    .await?;

    // Example: Get a property (like the state of the AC power)
    let state: u32 = proxy.get_property("State").await?;
    println!("AC power state: {}", state);

    Ok(())
}
