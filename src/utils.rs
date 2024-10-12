use futures::lock::Mutex;
use log::info;
use std::{collections::HashMap, env, path::PathBuf, process::Stdio};
use sysinfo::{ProcessExt, System, SystemExt};
use tokio::{process::Command, task::JoinHandle};
use xdg::BaseDirectories;

use super::config;

pub fn get_args(cmd: String) -> (String, Vec<String>) {
    let mut args = cmd.split_whitespace();
    let cmd = args.next().unwrap().to_string();
    let args: Vec<String> = args.map(|s| s.to_string()).collect();
    (cmd, args)
}

pub fn xdg_config_path(filename: Option<String>) -> std::io::Result<PathBuf> {
    let xdg_dirs = BaseDirectories::with_prefix(config::APP_NAME)?;

    match filename {
        Some(filename) => {
            let config_path: PathBuf = xdg_dirs.place_config_file(filename)?;
            return Ok(config_path);
        }
        None => Ok(xdg_dirs.get_config_home()),
    }
}

#[derive(Debug)]
pub struct Runner {
    tasks: Mutex<HashMap<String, JoinHandle<anyhow::Result<()>>>>,
}

impl Runner {
    pub fn new() -> Self {
        Self {
            tasks: Mutex::new(HashMap::new()),
        }
    }

    pub async fn run(&self, cmd: String) -> JoinHandle<Result<(), anyhow::Error>> {
        info!("cmd: {}", cmd);
        //TODO: get_args executed twice
        let (cmd, args) = get_args(cmd);

        tokio::spawn(async move {
            let mut child = Command::new(&cmd)
                .env(
                    "WAYLAND_DISPLAY",
                    env::var("WAYLAND_DISPLAY").unwrap_or_default(),
                )
                .env(
                    "DBUS_SESSION_BUS_ADDRESS",
                    env::var("DBUS_SESSION_BUS_ADDRESS").unwrap_or_default(),
                )
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .args(args)
                .spawn()
                .map_err(|e| {
                    anyhow::Error::msg(format!("Failed to spawn {} process: {}", cmd, e))
                })?;

            let status = child
                .wait()
                .await
                .map_err(|e| anyhow::Error::msg(format!("{} process failed to run: {}", cmd, e)))?;

            //{
            //    let mut tasks = self.tasks.lock().await;
            //    tasks.remove(&cmd);
            //}
            info!("Command {} completed with status: {:?}", cmd, status);

            Ok(())
        })
    }

    pub async fn run_once(&self, cmd: String) -> anyhow::Result<(), Box<dyn std::error::Error>> {
        let s = System::new_all();
        //TODO: get_args executed twice
        let (cmd_name, _) = get_args(cmd.clone());

        // Check if the process is already running
        let is_running = s
            .processes_by_exact_name(&cmd_name)
            .any(|p| p.name() == cmd_name);

        if !is_running {
            let mut tasks = self.tasks.lock().await;
            //if !tasks.contains_key(&cmd) {
            let _handle = self.run(cmd.clone()).await;
            //tasks.insert(cmd_name, handle);
            //}
        }
        Ok(())
    }
}
