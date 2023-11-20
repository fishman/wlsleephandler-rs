Helpers:log("Loading idle_config.lua")

function LockScreen()
  Helpers:log("Locking Screen")
  IdleNotifier:run_once("swaylock -f")
end

function LockHandler()
  IdleNotifier:run("playerctl -a pause")
  LockScreen()
end

function DpmsOn()
  Helpers:log("Turning screen on")
  IdleNotifier:run("swaymsg output '*' dpms on")
end

function DpmsOff()
  Helpers:log("Turning screen off")
  IdleNotifier:run("swaymsg output '*' dpms off")
end

function HybridSleep()
  IdleNotifier:run("systemctl hybrid-sleep")
end

function ScreenLockBattery(event)
  if Helpers:on_battery() == false then
    return
  end
  if event == "idled" then
    HybridSleep()
  end
end

function SleepBattery(event)
  if Helpers:on_battery() == false then
    return
  end
  if event == "idled" then
    LockScreen()
  end
end

function ScreenDpmsBattery(event)
  if Helpers:on_battery() == false then
    return
  end
  if event == "idled" then
    DpmsOff()
  elseif event == "resumed" then
    DpmsOn()
  end
end

function ScreenLockAC(event)
  if Helpers:on_battery() == true then
    return
  end
  if event == "idled" then
    LockScreen()
  end
end

function ScreenDpmsAC(event)
  if Helpers:on_battery() == true then
    return
  end
  if event == "idled" then
    DpmsOff()
  elseif event == "resumed" then
    DpmsOn()
  end
end

DbusHandler:PrepareSleep("LockScreen")
DbusHandler:LockHandler("LockHandler")
-- DbusHandler:UnlockHandler("UnlockHandler")
IdleNotifier:get_notification(300,  "ScreenLockBattery")
IdleNotifier:get_notification(30,  "ScreenDpmsBattery")
IdleNotifier:get_notification(600,  "SleepBattery")
IdleNotifier:get_notification(300,  "ScreenLockAC")
IdleNotifier:get_notification(600,  "ScreenDpmsAC")

Helpers:log("Finished loading idle_config.lua")
