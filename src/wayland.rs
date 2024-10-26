use mlua::Function;

use log::{debug, info};
use uuid::Uuid;
use wayland_client::{
    protocol::{
        wl_compositor, wl_output, wl_registry, wl_seat,
        wl_surface::{self},
    },
    Connection, Dispatch, QueueHandle,
};
use wayland_protocols::{
    ext::idle_notify::v1::client::{ext_idle_notification_v1, ext_idle_notifier_v1},
    wp::idle_inhibit::zv1::client::{
        zwp_idle_inhibit_manager_v1,
        zwp_idle_inhibitor_v1::{self},
    },
    xdg::activation::v1::client::{xdg_activation_token_v1, xdg_activation_v1},
};
use wayland_protocols_wlr::gamma_control::v1::client::{
    zwlr_gamma_control_manager_v1, zwlr_gamma_control_v1,
};

use crate::{color::Color, lua_init, types::State, INHIBIT_MANAGER, SURFACE};

#[derive(Debug)]
pub struct Output {
    reg_name: u32,
    wl_output: wl_output::WlOutput,
    name: Option<String>,
    color: Color,
    ramp_size: usize,
    color_changed: bool,
}

#[derive(Clone, Debug)]
pub struct NotificationContext {
    pub uuid: Uuid,
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
