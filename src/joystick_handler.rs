use evdev::{Device, InputEventKind};
use log::{debug, info};
use std::path::Path;

#[derive(Debug)]
pub struct JoystickHandler {
    syspath: String,
}

impl JoystickHandler {
    pub fn new(syspath: String) -> Self {
        Self { syspath }
    }

    pub async fn js_handler(&self) -> anyhow::Result<()> {
        let device_path = Path::new("/dev/input").join(&self.syspath);
        let device = Device::open(device_path)?;
        //let absinfo = device.get_abs_state();
        let mut event_stream = device.into_event_stream()?;

        loop {
            tokio::select! {
                ev = event_stream.next_event() => {
                    match ev {
                        Ok(ev) => {
                            match ev.kind() {
                                InputEventKind::Key(key) => {
                                    info!("Key event: {:?}, value: {}", key, ev.value());
                                }
                                // Ignore axis and synchronization events for now. For Axis events
                                // it's not currently clear how to get absinfo
                                InputEventKind::AbsAxis(..) => {
                                }
                                InputEventKind::Synchronization(..) =>  {}
                                _ => {
                                    debug!("Other event: {:?}", ev);
                                }
                            }
                        }
                        Err(e) => {
                            info!("Error reading event: {:?}", e);
                            break Ok(());
                        }
                    }
                }
            }
        }
    }
}
