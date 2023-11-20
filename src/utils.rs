use std::path::PathBuf;
use sysinfo::{ProcessExt, System, SystemExt};
use tokio::process::Command;
use xdg::BaseDirectories;

use super::config;

fn get_args(cmd: String) -> (String, Vec<String>) {
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

pub async fn run(cmd: String) -> anyhow::Result<(), Box<dyn std::error::Error>> {
    //TODO: get_args executed twice
    let (cmd, args) = get_args(cmd);

    let mut child = Command::new(&cmd)
        .args(args)
        .spawn()
        .expect(&format!("Failed to spawn {} process", cmd));

    // Wait for the process to complete to avoid a defunct process
    let _ = child
        .wait()
        .await
        .expect(&format!("{} process failed to run", cmd));

    Ok(())
}

pub async fn run_once(cmd: String) -> anyhow::Result<(), Box<dyn std::error::Error>> {
    let s = System::new_all();
    //TODO: get_args executed twice
    let (cmd_name, _) = get_args(cmd.clone());

    // Check if the process is already running
    let is_running = s
        .processes_by_exact_name(&cmd_name)
        .any(|p| p.name() == cmd_name);

    if !is_running {
        let _ = run(cmd.clone()).await;
    }
    Ok(())
}
