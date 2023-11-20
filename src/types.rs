#[derive(Debug)]
pub enum Request {
    LuaReload,
    LuaMethod(String),
    Reset,
    RunOnce(String),
    OnBattery(bool),
}
