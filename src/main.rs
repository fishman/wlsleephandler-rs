use clap::Parser;
use mlua::{Function, Lua, UserData, UserDataMethods};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::println;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use sysinfo::{ProcessExt, System, SystemExt};
// use tokio::process::Command;
use uuid::Uuid;
use wayland_client::protocol::{wl_registry, wl_seat};
use wayland_client::{Connection, Dispatch, EventQueue, QueueHandle};
use wayland_protocols::ext::idle_notify::v1::client::{
    ext_idle_notification_v1, ext_idle_notifier_v1,
};
use xdg::BaseDirectories;

const APP_NAME: &str = "swayidle-rs";
const CONFIG_FILE: &str = include_str!("../lua_configs/idle_config.lua");

fn run_once(command: &str) {
    let mut s = System::new_all();
    // Check if 'swaylock' is already running

    let is_running = s
        .processes_by_exact_name("swaylock")
        .any(|p| p.name() == "swaylock");
    // let is_running = s.processes().values().any(|p| p.name() == "swaylock");

    if !is_running {
        println!("swaylock is ");
        let mut command = Command::new("swaylock");
        command
            .args(["-f"])
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        unsafe {
            command.pre_exec(|| {
                // Ignore SIGINT and SIGTERM signals
                // libc::signal(libc::SIGINT, libc::SIG_IGN);
                // libc::signal(libc::SIGTERM, libc::SIG_IGN);
                Ok(())
            });
        }
        command.spawn().expect("failed to execute process");
    } else {
        println!("swaylock is already running");
    }
}

fn ensure_config_file_exists(filename: &str) -> std::io::Result<()> {
    let xdg_dirs = BaseDirectories::with_prefix(APP_NAME)?;
    let config_path: PathBuf = xdg_dirs.place_config_file(filename)?;

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
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value = "idle_config.lua")]
    config: String,
}

#[derive(Debug)]
struct State {
    wl_seat: Option<wl_seat::WlSeat>,
    qh: QueueHandle<State>,
    idle_notifier: Option<ext_idle_notifier_v1::ExtIdleNotifierV1>,
    notification_list: NotificationListHandle,
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
    }
}

fn generate_uuid() -> uuid::Uuid {
    Uuid::new_v4()
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let conn = Connection::connect_to_env().unwrap();
    let mut event_queue: EventQueue<State> = conn.new_event_queue();
    let qhandle = event_queue.handle();

    let display = conn.display();
    display.get_registry(&qhandle, ());

    let map: HashMap<Uuid, (String, ext_idle_notification_v1::ExtIdleNotificationV1)> =
        HashMap::new();
    let shared_map = Arc::new(Mutex::new(map));

    let mut state = State {
        wl_seat: None,
        idle_notifier: None,
        qh: qhandle.clone(),
        notification_list: shared_map.clone(),
        lua: Lua::new(),
    };

    let _ = ensure_config_file_exists("idle_config.lua");
    // Run the event loop in a separate async task
    // task::spawn(async move {
    //     loop {
    //         let _ = event_queue.dispatch_pending(&mut state).unwrap();
    //         tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    //     }
    // });
    // task::spawn(async move {
    //     loop {
    //         let _ = event_queue.dispatch
    //     }
    // });
    // let dbus_runtime = tokio::runtime::Runtime::new()?;
    // dbus_runtime.block_on(listen_for_sleep_signal())?;

    loop {
        event_queue.blocking_dispatch(&mut state).unwrap();
    }
}

fn _create_notifications(state: &mut State, qh: &QueueHandle<State>) {
    for i in 1..10 {
        let userdatarequest = NotificationContext {
            uuid: generate_uuid(),
        };
        let _notification = state.idle_notifier.as_ref().unwrap().get_idle_notification(
            i * 1000,
            state.wl_seat.as_ref().unwrap(),
            &qh,
            userdatarequest,
        );
    }
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
    };

    let globals = state.lua.globals();
    globals.set("IdleNotifier", my_lua_functions)?;

    let xdg_dirs = BaseDirectories::with_prefix(APP_NAME)?;
    let config_path: PathBuf = xdg_dirs.place_config_file(args.config)?;
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
                    state.wl_seat = Some(wl_seat);
                    println!("Seat: {:?}", name);

                    let _ = lua_init(state);
                }
                "ext_idle_notifier_v1" => {
                    let idle_notifier = registry
                        .bind::<ext_idle_notifier_v1::ExtIdleNotifierV1, _, _>(name, 1, qh, ());

                    println!("Idle Notifier: {:?}", name);
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
        println!("Idle Notification: {:?} {:?}", event, ctx.uuid);
        let map = state.notification_list.lock().unwrap();
        let globals = state.lua.globals();
        let fn_name = map.get(&ctx.uuid).unwrap().0.clone();
        let tostring: Function = globals.get(fn_name).unwrap();
        let _ = tostring.call::<_, ()>(match event {
            ext_idle_notification_v1::Event::Idled => "idled",
            ext_idle_notification_v1::Event::Resumed => "resumed",
            _ => "unknown",
        });
        run_once("swaylock");
    }
}

// async fn listen_for_sleep_signal() -> anyhow::Result<()> {
//     // Establish a connection to the D-Bus system bus
//     let connection = zbus::Connection::system().await?;

//     // Create a proxy to the login1 Manager interface
//     let proxy = connection.
//         .proxy(
//             "org.freedesktop.login1",
//             "/org/freedesktop/login1",
//             std::time::Duration::from_secs(30),
//         )
//         .await?;

//     // Listen for the PrepareForSleep signal
//     proxy
//         .connect_signal(|_: (bool,)| {
//             println!("Received PrepareForSleep signal");
//             // Execute swaylock here
//             // Note: You might want to handle this asynchronously or in a separate thread
//             if let Err(e) = Command::new("swaylock").spawn() {
//                 eprintln!("Failed to execute swaylock: {}", e);
//             }
//             Ok(())
//         })
//         .await?;

//     Ok(())
// }
