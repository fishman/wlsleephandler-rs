#[derive(Debug)]
pub enum Request {
    LuaReload,
    LuaMethod(String),
    Reset,
    Run(String),
    RunOnce(String),
    OnBattery(bool),
}
