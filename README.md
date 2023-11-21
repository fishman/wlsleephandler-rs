# swayidle-rs

This is intended as a replacement of sway's idle management daemon. I use it as a tool to understand rust message passing and state management. It aims to have the following goals:

- automatic setup of xdg config folder
- lua scripting to define idle management with a lua config
- automatic script reloading
- handling of AC and battery based idle timeouts
- screen locking with systemd locking

I am now using it as a replacement for any ac and battery power daemon, since I can run tasks with it.

## Install

Until packaging is added you can install sleepwatcher using cargo.

`cargo install --git https://github.com/fishman/sleepwatcher-rs`

## Debug

sleepwatcher-rs uses `env_logger`. You can enable logging by setting the `RUST_LOG` environment variable:

`RUST_LOG=debug sleepwatcher-rs`

## Default config

The default config is written to `~/.config/sleepwatcher-rs/idle_config.lua` on startup if the folder and file does not exist yet.

## Syntax

Lua is configured to be sandboxed, so no library functions can be used and only functions exposed inside the Rust can be used.

Important distinction between `Helpers:run` and `Helpers:run_once`. `run_once` will check if a process of that name is already running and won't spawn a new one in that case. This may be useful, when a screen locker can create race conditions if spawned twice.

Originally I wanted to reload the config whenever the AC adaptor is plugged in and out, but due to the timeout issue described below, you can check for the `on_battery` state in functions.

``` lua
Helpers:log("Loading idle_config.lua")

function LockScreen()
  Helpers:log("Locking Screen")
  IdleNotifier:run_once("swaylock -f")
end

function ScreenLockBattery(event)
  if event == "idled" and Helpers:on_battery() then
    LockScreen()
  end
end

DbusHandler:PrepareSleep("LockScreen")
DbusHandler:LockHandler("LockHandler")
DbusHandler:UnlockHandler("UnlockHandler")
IdleNotifier:get_notification(300,  "ScreenLockBattery")
```

`get_notification` creates a Wayland idle timeout handler. It uses the `ext-idle-notify-v1` protocol. It is not (yet) possible to create callback functions, so the function calls are made by specifying the name of the function.

`PrepareSleep`, `LockScreen`, `UnlockScreen`, are dbus signals from the `org.freedesktop.logind.manager` and `org.freedesktop.logind.session`.

## Known issues

- sleepwatcher-rs should automatically reload the config when `~/.config/sleepwatcher-rs/idle_config.lua` is changed. However, due to an unknown reason the first trigger after reload still follows the old timeout and the next trigger is therefore equal to the rest of the previous timeout+the new timeout setting.

## TODOS

- [ ] build CI
- [ ] AUR package
- [ ] use more idiomatic lua syntax
- [ ] add tests
- [ ] enable callbacks for lua code. it currently complains about lua functions not supporting Send. It's related to the way the user methods are implemented
- [ ] duplicate state information
- [ ] find a way to remove the std::thread::spawn that used within the lua thread to run_once
- [ ] add idle hints
- [ ] improve return and error handling
- [ ] turn wayland_run spawn_blocking into async
