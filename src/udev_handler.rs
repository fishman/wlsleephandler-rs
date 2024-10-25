use log::debug;
use std::collections::HashMap;
use tokio::{
    io::unix::AsyncFd,
    sync::{mpsc, Mutex},
    task::JoinHandle,
};
use udev::{EventType, MonitorBuilder};

use crate::joystick_handler::JoystickHandler;
use crate::types::Request;

pub struct UdevHandler {
    joysticks: Mutex<HashMap<String, JoinHandle<anyhow::Result<()>>>>,
    tx: mpsc::Sender<Request>,
}

impl UdevHandler {
    pub fn new(tx: mpsc::Sender<Request>) -> Self {
        Self {
            joysticks: Mutex::new(HashMap::new()),
            tx,
        }
    }

    async fn joystick_add(&self, joystick: udev::Device) -> anyhow::Result<()> {
        if let Some(sysname) = self.get_joystick_sysname(joystick) {
            debug!("Added joystick {}", sysname.clone());
            let joystick_handler = JoystickHandler::new(sysname.clone(), self.tx.clone());
            let mut joysticks = self.joysticks.lock().await;
            joysticks.insert(
                sysname.clone(),
                tokio::spawn(async move { joystick_handler.js_handler().await }),
            );
        }
        Ok(())
    }

    async fn joystick_remove(&self, joystick: udev::Device) -> anyhow::Result<()> {
        if let Some(sysname) = self.get_joystick_sysname(joystick) {
            debug!("Removed joystick {}", sysname.clone());
            let mut joysticks = self.joysticks.lock().await;
            let task = joysticks.get(&sysname).unwrap();
            task.abort();
            joysticks.remove(&sysname);
        }
        Ok(())
    }

    fn _timer_event(&mut self) {
        // Handle timer event
        //if let Some(ref mut inhibitor) = self.idle_inhibitor {
        //    inhibitor.destroy();
        //    self.idle_inhibitor = None;
        //    self.wl_display.as_ref().unwrap().flush();
        //}
    }

    fn _log_device(&self, event: udev::Device) {
        debug!(
            "{}: (subsystem={}, sysname={}, devtype={})",
            event.sysname().to_str().unwrap_or("---"),
            event
                .subsystem()
                .map_or("", |s| { s.to_str().unwrap_or("") }),
            event.sysname().to_str().unwrap_or(""),
            event.devtype().map_or("", |s| { s.to_str().unwrap_or("") })
        );
    }

    fn _log_event(&self, event: udev::Event) {
        debug!(
            "{}: {} {} (subsystem={}, sysname={}, devtype={})",
            event.sequence_number(),
            event.event_type(),
            event.sysname().to_str().unwrap_or("---"),
            event
                .subsystem()
                .map_or("", |s| { s.to_str().unwrap_or("") }),
            event.sysname().to_str().unwrap_or(""),
            event.devtype().map_or("", |s| { s.to_str().unwrap_or("") })
        );
    }

    fn get_joystick_sysname(&self, device: udev::Device) -> Option<String> {
        if device.property_value("ID_INPUT_JOYSTICK").is_some() {
            if let Some(sysname) = device.sysname().to_str() {
                if sysname.starts_with("event") {
                    return Some(sysname.to_string());
                }
            }
        }
        None
    }

    async fn update_device(&self, event_type: EventType, device: udev::Device) {
        match event_type {
            EventType::Add => {
                let _ = self.joystick_add(device).await;
            }
            EventType::Remove => {
                let _ = self.joystick_remove(device).await;
            }
            _ => {}
        }
    }

    async fn iterate_joystick_devices(&self) -> anyhow::Result<()> {
        let mut enumerator = udev::Enumerator::new()?;
        let _ = enumerator.match_subsystem("input");
        let _ = enumerator.match_property("ID_INPUT_JOYSTICK", "1");
        let _ = enumerator.match_sysname("event*");

        for device in enumerator.scan_devices()? {
            let _ = self.update_device(EventType::Add, device).await;
        }

        Ok(())
    }

    pub async fn monitor(&self) -> anyhow::Result<()> {
        let socket = MonitorBuilder::new()?.match_subsystem("input")?.listen()?;

        let _ = self.iterate_joystick_devices().await;
        let async_socket = AsyncFd::new(socket)?;

        loop {
            tokio::select! {
                result = async_socket.readable() => {
                    let mut guard = result?;

                    while let Some(event) = async_socket.get_ref().iter().next() {
                        if let Some(_sysname) = self.get_joystick_sysname(event.clone()) {
                            self.update_device(event.event_type(), event.device()).await;
                        }
                    }

                    guard.clear_ready();
                }
            }
        }
    }
}
