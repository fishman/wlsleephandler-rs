use evdev::{Device, InputEventKind};
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
        let mut event_stream = device.into_event_stream()?;

        loop {
            tokio::select! {
                ev = event_stream.next_event() => {
                    match ev {
                        Ok(ev) => {
                            match ev.kind() {
                                InputEventKind::Key(key) => {
                                    println!("Key event: {:?}, value: {}", key, ev.value());
                                }
                                InputEventKind::AbsAxis(axis) => {
                                    println!("Axis event: {:?}, value: {}", axis, ev.value());
                                }
                                _ => {
                                    println!("Other event: {:?}", ev);
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Error reading event: {:?}", e);
                            break Ok(());
                        }
                    }
                }
            }
        }
    }
}
