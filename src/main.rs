use clap::Parser;
use mlua::{Function, Lua, UserData, UserDataMethods};
use std::collections::HashMap;
use std::{fs, println};
use std::sync::{Arc, Mutex};
use uuid::Uuid;
use wayland_client::protocol::{wl_registry, wl_seat};
use wayland_client::{Connection, Dispatch, EventQueue, QueueHandle};
use wayland_protocols::ext::idle_notify::v1::client::{
    ext_idle_notification_v1, ext_idle_notifier_v1,
};
// fn my_async_function(event_handler: &mut EventQueue<State>, qhandle: QueueHandle<State>, state: State) {
//
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
    qh: QueueHandle<State>,
    idle_notifier: Option<ext_idle_notifier_v1::ExtIdleNotifierV1>,
    notification_list: NotificationListHandle,
    lua: Lua,
}

#[derive(Clone, Debug)]
struct NotificationContext {
    uuid: Uuid,
    // idle_cb: Function,
}

struct MyLuaFunctions {
    wl_seat: Option<wl_seat::WlSeat>,
    qh: QueueHandle<State>,
    idle_notifier: Option<ext_idle_notifier_v1::ExtIdleNotifierV1>,
    notification_list: NotificationListHandle,
}

type StateHandle = Arc<Mutex<State>>;

type NotificationListHandle = Arc<
    Mutex<
        HashMap<
            Uuid,
            (
                String,
                ext_idle_notification_v1::ExtIdleNotificationV1,
            ),
        >,
    >,
>;

impl UserData for MyLuaFunctions {
    fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_method(
            "get_notification",
            |_lua, this, (timeout, fn_name): (i32, String)| {
                let ctx = NotificationContext {
                    uuid: generate_uuid(),
                };

                let notification = this
                    .idle_notifier
                    .as_ref()
                    .unwrap()
                    .get_idle_notification(
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
async fn main() -> mlua::Result<()> {
    let conn = Connection::connect_to_env().unwrap();
    let mut event_queue: EventQueue<State> = conn.new_event_queue();
    let qhandle = event_queue.handle();

    let display = conn.display();
    display.get_registry(&qhandle, ());

    let map: HashMap<
        Uuid,
        (
            String,
            ext_idle_notification_v1::ExtIdleNotificationV1,
        ),
    > = HashMap::new();
    let shared_map = Arc::new(Mutex::new(map));

    // let mut state = State {
    //     wl_seat: None,
    //     idle_notifier: None,
    //     qh: qhandle,
    //     // notification_list: shared_map,
    //     lua: Lua::new(),
    // };
    let mut state = State {
        wl_seat: None,
        idle_notifier: None,
        qh: qhandle.clone(),
        notification_list: shared_map.clone(),
        lua: Lua::new(),
    };
    {
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
        // let mut state = state.as_ref().lock().unwrap();
        event_queue.blocking_dispatch(&mut state).unwrap();
        // tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    // Your application logic goes here
    // ...
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

fn lua_init(state: &mut State) -> mlua::Result<()> {
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

    let config_path = args.config;
    let config_script = fs::read_to_string(config_path)?;

    let _result = lua.load(&config_script).exec()?;
    // match result {
    //     Ok(_) => println!("Lua config loaded successfully"),
    //     Err(e) => println!("Error loading Lua config: {}", e),
    // }

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

                    // create_notifications(state, &qh);
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
    }
}
