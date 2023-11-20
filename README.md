# swayidle-rs

This is intended as a replacement of sway's idle management daemon. I use it as a tool to understand rust message passing and state management. It aims to have the following goals:

- automatic setup of xdg config folder
- lua scripting to define idle management with a lua config
- automatic script reloading
- handling of AC and battery based idle timeouts
- screen locking with systemd locking


## TODOS

- [ ] use more idiomatic lua syntax
- [ ] add tests
- [ ] enable callbacks for lua code. it currently complains about lua functions not supporting Send. It's related to the way the user methods are implemented
- [ ] duplicate state information
- [ ] find a way to remove the std::thread::spawn that used within the lua thread to run_once
