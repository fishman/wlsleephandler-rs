use clap::Parser;
use mlua::{Lua, UserData, UserDataMethods, Function};
use tokio::sync::mpsc;
use std::{fs, print};
use std::sync::{Arc, Mutex};
use wayland_client::{Connection, Dispatch, QueueHandle, EventQueue};
use wayland_client::protocol::{wl_registry, wl_seat};
use  wayland_protocols::ext::idle_notify::v1::client::{ext_idle_notifier_v1, ext_idle_notification_v1};
use tokio::task;
use std::task::{Poll, Context};
use std::pin::Pin;
use std::convert::Infallible;

// fn my_async_function(event_handler: &mut EventQueue<State>, qhandle: QueueHandle<State>, state: State) {
//     let mut context = Context::from_waker(futures::task::noop_waker_ref());
//
//     loop {
//         match Pin::new(&mut event_handler).poll_dispatch_pending(&mut context, &mut state) {
//             // Poll::Ready(Infallible::from(Ok(())) => {
//             //     // All pending events have been processed
//             //     // return Ok::<(), Infallible>(());
//             // },
//             Poll::Ready(Err(e)) => {
//                 // Handle errors here
//             },
//             Poll::Pending => {
//                 // No events to process, can yield to other tasks
//                 break;
//             }
//         }
//     }
// }

// mod idle_notifier;

#[derive(Debug)]
pub enum Request {
    Reload,
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[arg(short, long, default_value = "lua_configs/idle_config.lua")]
    config: String,
}

#[tokio::main]
async fn main() -> mlua::Result<()> {
    let conn = Connection::connect_to_env().unwrap();
    let mut event_queue: EventQueue<State> = conn.new_event_queue();
    let qhandle = event_queue.handle();

    let display = conn.display();
    display.get_registry(&qhandle, ());

    let mut state = State {
        wl_seat: None,
        idle_notifier: None,
    };

    // Run the event loop in a separate async task
    // task::spawn(async move {
    //     loop {
    //         let _ = event_queue.dispatch_pending(&mut state).unwrap();
    //         tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    //     }
    // });

    println!("Starting the example window app, press <ESC> to quit.");

    // while true {
    //     event_queue.blocking_dispatch(&mut state).unwrap();
    // }
    // Your application logic goes here
    // ...

    Ok(())
}

struct State {
    wl_seat: Option<wl_seat::WlSeat>,
    idle_notifier: Option<ext_idle_notifier_v1::ExtIdleNotifierV1>,
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        _: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global { name, interface, .. } = event {
            println!("Global: {:?}", interface);
            match &interface[..] {
                "wl_seat" => {
                    registry.bind::<wl_seat::WlSeat, _, _>(name, 1, qh, ());
                    println!("Seat: {:?}", name);
                }
                "ext_idle_notifier_v1" => {
                    println!("idle: {:?}", name);
                    registry.bind::<ext_idle_notifier_v1::ExtIdleNotifierV1, _, _>(name, 1, qh, ());
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for State {
    fn event(
        state: &mut Self,
        seat: &wl_seat::WlSeat,
        _event: wl_seat::Event,
        _: &(),
        _: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        state.wl_seat = Some(seat.clone());
        println!("Seat:");
    }
}

impl Dispatch<ext_idle_notifier_v1::ExtIdleNotifierV1, ()> for State {
    fn event(
        state: &mut Self,
        idle_notifier: &ext_idle_notifier_v1::ExtIdleNotifierV1,
        event: ext_idle_notifier_v1::Event,
        _: &(),
        _: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        println!("Idle Notifier: {:?}", event);
        state.idle_notifier = Some(idle_notifier.clone());
    }
}

