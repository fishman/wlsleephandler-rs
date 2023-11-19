#[derive(Debug)]
pub enum Request {
    LuaReload,
    Reset,
    RunOnce(String),
    BatteryState(bool),
}
