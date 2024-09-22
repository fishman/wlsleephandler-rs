use log::info;
use nix::fcntl;
use nix::sys::stat::Mode;
use std::{
    io::ErrorKind,
    os::unix::{
        io::{AsRawFd, BorrowedFd, FromRawFd, RawFd},
        net::UnixDatagram as StdUnixDatagram,
    },
    sync::Arc,
    time::Duration,
};
use tokio::{
    net::UnixDatagram as TokioUnixDatagram,
    signal::unix::{signal, SignalKind},
    sync::Mutex,
    time::sleep,
};
use udev::{Device, Enumerator, MonitorBuilder, MonitorSocket};

const JOYSTICKS_MAX: usize = 5;
const JOYSTICKS_FD_START: usize = 3;
const FDS_MAX: usize = JOYSTICKS_FD_START + JOYSTICKS_MAX;
static mut FDS: [RawFd; FDS_MAX] = [-1; FDS_MAX];

struct Joystick<'a> {
    fd: BorrowedFd<'a>,
    // other joystick-specific data
}

pub struct JoystickHandler {
    paused: bool,
    timeout_sec: u64,
    joysticks: Vec<Joystick<'static>>,
    // other necessary state
}

impl JoystickHandler {
    pub fn new() -> Self {
        Self {
            paused: false,
            timeout_sec: 30,
            joysticks: vec![],
        }
    }

    fn add_device(&mut self, dev: &Device) {
        let path = dev.syspath().to_string_lossy().to_string();
        if !path.ends_with("/event") {
            return;
        }

        if let Some(joystick) = dev.property_value("ID_INPUT_JOYSTICK") {
            if joystick != "1" {
                return;
            }
        }

        let inputdev = format!("/dev/input{}", path.rsplit('/').next().unwrap());

        for i in 0..JOYSTICKS_MAX {
            let f = JOYSTICKS_FD_START + i;
            if unsafe { FDS[f] } >= 0 {
                continue;
            }
            println!("Adding joystick {}", inputdev);
            match fcntl::open(
                inputdev.as_str(),
                fcntl::OFlag::O_RDONLY | fcntl::OFlag::O_NONBLOCK,
                Mode::empty(),
            ) {
                Ok(fd) => {
                    println!("{} => [{}]", inputdev, fd);
                    unsafe {
                        FDS[f] = fd;

                        let borrowed_fd: BorrowedFd<'_> = BorrowedFd::borrow_raw(fd);
                        self.joysticks.push(Joystick { fd: borrowed_fd });
                    }
                    return;
                }
                Err(err) => {
                    eprintln!("Error opening {}: {}", inputdev, err);
                    return;
                }
            }
        }
    }

    fn timer_event(&mut self) {
        // Handle timer event
        //if let Some(ref mut inhibitor) = self.idle_inhibitor {
        //    inhibitor.destroy();
        //    self.idle_inhibitor = None;
        //    self.wl_display.as_ref().unwrap().flush();
        //}
    }

    pub async fn udev_handler_run(app_state: Arc<Mutex<Self>>) {
        let _app_state_clone = Arc::clone(&app_state);
    }

    //async fn process_device_events() -> std::io::Result<()> {
    //    let monitor = MonitorBuilder::new()?
    //        .match_subsystem("input")? // how to listen only to joystick events?
    //        .listen()?;
    //
    //    let fd = monitor.as_raw_fd();
    //    let std_socket = unsafe { StdUnixDatagram::from_raw_fd(fd) };
    //    let socket = TokioUnixDatagram::from_std(std_socket)?;
    //
    //    let socket = Arc::new(socket);
    //
    //    tokio::spawn(async move {
    //        let mut buffer = [0u8; 1024];
    //
    //        loop {
    //            match socket.recv(&mut buffer).await {
    //                Ok(_) => {
    //                    let event =
    //                        udev::Event // how do i get the event?
    //
    //                    let action = event.action().to_string_lossy();
    //                    let devpath = event.devpath().to_string_lossy();
    //                    let devnode = event
    //                        .devnode()
    //                        .map(|d| d.to_string_lossy())
    //                        .unwrap_or("unknown".into());
    //
    //                    println!(
    //                        "Event: action={}, devpath={}, devnode={}",
    //                        action, devpath, devnode
    //                    );
    //                }
    //                Err(e) if e.kind() == ErrorKind::WouldBlock => {
    //                    // Continue waiting for new events.
    //                    continue;
    //                }
    //                Err(e) => {
    //                    eprintln!("Error receiving device event: {}", e);
    //                    break;
    //                }
    //                Err(_) => {
    //                    continue;
    //                }
    //            }
    //        }
    //    });
    //
    //    Ok(())
    //}

    pub async fn run(app_state: Arc<Mutex<Self>>) {
        let app_state_clone = Arc::clone(&app_state);
        let mut sigusr1 = signal(SignalKind::user_defined1()).unwrap();
        let mut sigusr2 = signal(SignalKind::user_defined2()).unwrap();

        tokio::spawn({
            async move {
                loop {
                    let mut app_state = app_state_clone.lock().await;
                    tokio::select! {
                        //_ = events[0].readable() => {
                        //},
                        _ = sigusr1.recv() => {
                            app_state.paused = true;
                        }
                        _ = sigusr2.recv() => {
                            app_state.paused = false;
                        }
                        _ = sleep(Duration::from_secs(app_state.timeout_sec)) => {
                            if !app_state.paused {
                                // Handle timeout
                            }
                        }
                    }
                }
            }
        });

        // Main event loop
        loop {
            let app_state = app_state.lock().await;
            for _joystick in &app_state.joysticks {
                // Create BorrowedFd from raw fd when needed
                // Read from joystick, handle events
                info!("handle joystick event");
            }
        }
    }
}
