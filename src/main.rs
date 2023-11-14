use clap::Parser;
use mlua::{Lua, UserData, UserDataMethods, Function};
use tokio::sync::mpsc;
use wayrs_protocols::ext_idle_notify_v1::{ExtIdleNotificationV1, ExtIdleNotifierV1};
use std::{fs, print};

use wayrs_protocols::ext_idle_notify_v1::*;
use wayrs_client::protocol::wl_seat::WlSeat;
use wayrs_client::Connection;
use wayrs_client::EventCtx;

// mod idle_notifier;

#[derive(Debug)]
pub enum Request {
    SetTimeout(i32),
}

#[derive(Debug)]
struct SharedState {
    idle_notifier: ExtIdleNotifierV1,
    idle_notifications: Vec<ExtIdleNotificationV1>,
}


pub async fn run(mut rx: mpsc::Receiver<Request>) -> Result<(), Box<dyn std::error::Error>> {
    let (mut conn, globals) = Connection::async_connect_and_collect_globals().await?;

    let registry = conn.registry();
    let seat: WlSeat = globals.iter()
        .find(|global| global.interface.to_str().unwrap() == "wl_seat")
        .map(|seat_global| {
            // Bind to the wl_seat
            registry.bind(&mut conn, seat_global.name, seat_global.version)
        })
        .expect("wl_seat not found");

    let idle_notifier: ExtIdleNotifierV1 = globals.iter()
        .find(|global| global.interface.to_str().unwrap() == "ext_idle_notifier_v1")
        .map(|idle_notifier_global| {
            // Bind to the wl_seat
            registry.bind(&mut conn, idle_notifier_global.name, idle_notifier_global.version)
        })
        .expect("idle_notifier not found");

    let mut state = SharedState {
        idle_notifier,
        idle_notifications: Vec::new(),
    };

    let idle_notification = state.idle_notifier.get_idle_notification_with_cb(
        &mut conn, 
        10000, 
        seat, 
        |ctx: EventCtx<SharedState, ExtIdleNotificationV1>| {
            match ctx.event {
                ext_idle_notification_v1::Event::Idled => {
                    println!("Idled");
                }
                ext_idle_notification_v1::Event::Resumed => {
                    println!("Resumed");
                }
                _ => {}
            }
        }
    );

    state.idle_notifications.push(idle_notification);

    loop {
        conn.async_flush().await?;

        tokio::select! {
            recv_events = conn.async_recv_events() => {
                recv_events?;
                conn.dispatch_events(&mut state);
            }
            Some(request) = rx.recv() => {
                println!("Received request: {:?}", request);
            }
        }
    }
}

fn idle_notification_cb(lua_cb: Function, ctx: EventCtx<SharedState, ExtIdleNotificationV1>) -> mlua::Result<()> {
    // You will need to map the Rust event context to a format that Lua can understand.
    // This example assumes you've created a way to map `ctx.event` to a Lua value.
    // let event_value = map_event_ctx_to_lua_value(ctx)?;
    //
    // // Call the Lua function with the event value.
    // lua_cb.call::<_, ()>(event_value)?;
    Ok(())
}

struct MyLuaFunctions;

impl UserData for MyLuaFunctions {
    fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_function("idle_notification", |lua, (timeout, lua_cb): (i32, Function)| {
            // let idle_notifier = // Initialize or retrieve your idle_notifier instance

            // let _idle_notification = idle_notifier.get_idle_notification_with_cb(
            //     &mut conn, 
            //     timeout, 
            //     seat, 
            //     move |ctx| idle_notification_cb(lua_cb.clone(), ctx)
            // );

            Ok(())
        });
    }
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value = "lua_configs/idle_config.lua")]
    config: String,
}

#[tokio::main]
async fn main() -> mlua::Result<()> {
// fn main() -> mlua::Result<()> {
    let (_tx, rx) = mpsc::channel(32);

    let args = Args::parse();

    // let shared_state = Arc::new(Mutex::new(SharedState { counter: 0 }));
    let lua = Lua::new();
    lua.sandbox(true)?;

    let config_path = args.config;
    let config_script = fs::read_to_string(config_path)?;
    let config: mlua::Table = lua.load(&config_script).eval()?;

    // Retrieve settings from Lua config
    let _idle_duration = config.get::<_, u64>("idle_duration")?;
    let _idle_script = config.get::<_, String>("idle_script")?;

    print!("idle_duration = {}\n", _idle_duration);


    // let rt = runtime::Builder::new_multi_thread()
    //     .enable_all()
    //     .build()
    //     .unwrap();
    //
    //

    let handle = tokio::spawn(async{
        let _ = run(rx).await.map_err(|e| mlua::Error::external(format!("An unknown error occurred: {}", e)));
    });

    match handle.await {
        Ok(_) => println!("Task completed successfully"),
        Err(e) => println!("Task failed: {:?}", e),
    }

    Ok(())
}
