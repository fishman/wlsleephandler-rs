#[derive(Debug)]
pub enum Request {
    LuaReload,
    LuaMethod(String),
    Reset,
    OnBattery(bool),
}
