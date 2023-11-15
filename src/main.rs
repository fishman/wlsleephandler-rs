use clap::Parser;
use mlua::{Lua, UserData, UserDataMethods, Function};
use tokio::sync::mpsc;
use wayrs_protocols::ext_idle_notify_v1::{ExtIdleNotificationV1, ExtIdleNotifierV1};
use std::{fs, print};
use std::sync::{Arc, Mutex};

use wayrs_protocols::ext_idle_notify_v1::*;
use wayrs_client::protocol::wl_seat::WlSeat;
use wayrs_client::Connection;
use wayrs_client::EventCtx;

// mod idle_notifier;

#[derive(Debug)]
pub enum Request {
    Reload,
}

#[derive(Debug)]
struct State {
    idle_notifier: ExtIdleNotifierV1,
    idle_notifications: Vec<ExtIdleNotificationV1>,
}

#[derive(Debug)]
pub struct LuaState {
    lua: Lua,
}

struct LuaStateNew { }

type LuaStateHandle = Arc<Mutex<LuaState>>;


fn lua_init(lua: &Lua, tx: mpsc::Sender<i32>) -> Result<(), Box<dyn std::error::Error>> {
    lua.sandbox(true)?;
    let args = Args::parse();

    let config_path = args.config;
    lua.globals().set("get_idle_notification", create_idle_notification_fn(lua, tx.clone())?)?;
    let config_script = fs::read_to_string(config_path)?;
    let config: mlua::Table = lua.load(&config_script).eval()?; 

    // Retrieve settings from Lua config
    let _idle_duration = config.get::<_, u64>("idle_duration")?;
    let _idle_script = config.get::<_, String>("idle_script")?;

    print!("idle_duration = {}\n", _idle_duration);

    Ok(())
}

// pub fn create_idle_notification(
//     idle_notifier: ExtIdleNotifierV1,
//     conn: &mut Connection<State>,
//     seat: WlSeat) -> ExtIdleNotificationV1 {
//     return idle_notifier.get_idle_notification_with_cb(
//         conn, 
//         10000, 
//         seat, 
//         |ctx: EventCtx<State, ExtIdleNotificationV1>| {
//             match ctx.event {
//                 ext_idle_notification_v1::Event::Idled => {
//                     println!("Idled");
//                 }
//                 ext_idle_notification_v1::Event::Resumed => {
//                     println!("Resumed");
//                 }
//                 _ => {}
//             }
//         }
//     );
// }

pub async fn run(mut rx: mpsc::Receiver<Request>) -> Result<(), Box<dyn std::error::Error>> {
    let (mut conn, globals) = Connection::async_connect_and_collect_globals().await?;


    {
        // let lua = &shared_state.lock().unwrap().lua;

        let lua = Lua::new();
        let _ = lua_init(&lua);
    }


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

    let mut state = State {
        idle_notifier,
        idle_notifications: Vec::new(),
    };

    // let idle_notification = create_idle_notification(state.idle_notifier, conn, seat);
    let idle_notification = idle_notifier.get_idle_notification_with_cb(
        &mut conn, 
        10000, 
        seat, 
        |ctx: EventCtx<State, ExtIdleNotificationV1>| {
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
                match request {
                    Request::Reload => {
                        println!("Reloading config");
                    }
                }
            }
        }
    }
}


struct SenderWrapper(mpsc::Sender<i32>);
    // shared_state: LuaStateHandle,

impl UserData for SenderWrapper {}

pub async fn create_idle_notification() -> Result<(), Box<dyn std::error::Error>> {
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

    let mut state = LuaStateNew { };

    loop {
        conn.async_flush().await?;

        tokio::select! {
            recv_events = conn.async_recv_events() => {
                recv_events?;
                conn.dispatch_events(&mut state);
            }
        }
    }
}

fn idle_notification_cb(lua_cb: Function, ctx: EventCtx<State, ExtIdleNotificationV1>) -> mlua::Result<()> {
    // You will need to map the Rust event context to a format that Lua can understand.
    // This example assumes you've created a way to map `ctx.event` to a Lua value.
    // let event_value = map_event_ctx_to_lua_value(ctx)?;
    //
    // // Call the Lua function with the event value.
    // lua_cb.call::<_, ()>(event_value)?;
    Ok(())
}

fn create_idle_notification_fn(lua: &Lua, tx: mpsc::Sender<i32>) -> mlua::Result<Function> {
    let sender = SenderWrapper(tx);

    lua.create_function(move |_, notification: String| {
        let _sender = sender.0.clone();
        tokio::spawn(async move {
            let _ = create_idle_notification().await.unwrap();
            // sender.send(message).await.unwrap();
        });
        Ok(())
    })
}
    // fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(methods: &mut M) {
    //     methods.add_function("idle_notification", |lua, (timeout, lua_cb): (i32, Function)| {
            // let idle_notifier = // Initialize or retrieve your idle_notifier instance

            // let _idle_notification = idle_notifier.get_idle_notification_with_cb(
            //     &mut conn, 
            //     timeout, 
            //     seat, 
            //     move |ctx| idle_notification_cb(lua_cb.clone(), ctx)
            // );

//             Ok(())
//         });
//     }
// }

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


    // let shared_state = Arc::new(Mutex::new(LuaState {
    //     lua: Lua::new(),
    // }));
    //

    // let lua = &shared_state.lock().unwrap().lua;
    // lua.sandbox(true)?;

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
