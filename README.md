# swayidle-rs

This is intended as a replacement of sway's idle management daemon. It aims to have the following goals:

- automatic setup of xdg config folder
- lua scripting to define idle management with a lua config
- automatic script reloading
- handling of AC and battery based idle timeouts
- screen locking with systemd locking
