use clap::Parser;
use mlua::Lua;
use tokio::sync::mpsc;
use wayrs_protocols::ext_idle_notify_v1::{ExtIdleNotificationV1, ExtIdleNotifierV1};
use std::{fs, print};

use wayrs_protocols::ext_idle_notify_v1::*;
use wayrs_client::protocol::wl_seat::WlSeat;
use wayrs_client::Connection;
use wayrs_client::EventCtx;

#[derive(Debug)]
pub enum Request {
    SetTimeout(i32),
}

#[derive(Debug)]
struct State {
    idle_notification: ExtIdleNotificationV1,
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

    for global in globals {
        println!(
            "{} v{}",
            global.interface.into_string().unwrap(),
            global.version
        );
    }

    //
    let mut state = State {
        idle_notification: idle_notification
    };
    //


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

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value = "lua_configs/idle_config.lua")]
    config: String,
}

#[tokio::main]
async fn main() -> mlua::Result<()> {
// fn main() -> mlua::Result<()> {
    // let (_tx, rx) = mpsc::channel::<T>(32);
    // let (mut tx, mut rx) = mpsc::channel(32);

    let args = Args::parse();

    let lua = Lua::new();

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
    use tokio::time::{sleep, Duration};
    let (_tx, rx) = mpsc::channel(16);

    run(rx).await.map_err(|e| mlua::Error::external(format!("An unknown error occurred: {}", e)))?;

    // tokio::task::spawn(async move {
    //     loop {
    //         // Handle Wayland events
    //         // The call to roundtrip is a blocking call
    //     }
    // });

    Ok(())
}
