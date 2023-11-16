use clap::Parser;
use mlua::{Function, Lua, UserData, UserDataMethods};
use wayland_client::protocol::{wl_registry, wl_seat};
use wayland_client::{Connection, Dispatch, EventQueue, QueueHandle};
use wayland_protocols::ext::idle_notify::v1::client::{
    ext_idle_notification_v1, ext_idle_notifier_v1,
};

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

#[derive(Debug)]
struct State {
    wl_seat: Option<wl_seat::WlSeat>,
    idle_notifier: Option<ext_idle_notifier_v1::ExtIdleNotifierV1>,
    lua: Lua,
}

#[derive(Debug)]
struct NotificationContext {
    id: u32,
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
        lua: Lua::new(),
    };

    {
        state.lua.sandbox(true)?;
    }

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

    loop {
        event_queue.blocking_dispatch(&mut state).unwrap();
        // tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    // Your application logic goes here
    // ...
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
                    for i in 1..10 {
                        let userdatarequest = NotificationContext { id: i };
                        let _notification =
                            state.idle_notifier.as_ref().unwrap().get_idle_notification(
                                i * 1000,
                                state.wl_seat.as_ref().unwrap(),
                                &qh,
                                userdatarequest,
                            );
                    }
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
        _state: &mut Self,
        _idle_notification: &ext_idle_notification_v1::ExtIdleNotificationV1,
        event: ext_idle_notification_v1::Event,
        udata: &NotificationContext,
        _: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        println!("Idle Notification: {:?} {:?}", event, udata.id);
    }
}
