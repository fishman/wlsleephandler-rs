use std::path::PathBuf;
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
            Ok(config_path)
        }
        None => Ok(xdg_dirs.get_config_home()),
    }
}
