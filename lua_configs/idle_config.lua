IdleNotifier:log("Loading idle_config.lua")
function LockScreen()
  IdleNotifier:log("Locking Screen")
  IdleNotifier:run_once("swaylock -f")
end

function DpmsOn()
  IdleNotifier:log("Turning screen on")
  IdleNotifier:run_once("swaymsg output '*' dpms on")
end

function DpmsOff()
  IdleNotifier:log("Turning screen off")
  IdleNotifier:run_once("swaymsg output '*' dpms off")
end

function ScreenLockBattery(event)
  if Settings:on_battery() == false then
    return
  end
  if event == "idled" then
    LockScreen()
  end
end

function ScreenDpmsBattery(event)
  if Settings:on_battery() == false then
    return
  end
  if event == "idled" then
    DpmsOff()
  elseif event == "resumed" then
    DpmsOn()
  end
end

function ScreenLockAC(event)
  if Settings:on_battery() == true then
    return
  end
  if event == "idled" then
    LockScreen()
  end
end

function ScreenDpmsAC(event)
  if Settings:on_battery() == true then
    return
  end
  if event == "idled" then
    DpmsOff()
  elseif event == "resumed" then
    DpmsOn()
  end
end


IdleNotifier:get_notification(60,  "ScreenLockBattery")
IdleNotifier:get_notification(10,  "ScreenDpmsBattery")
IdleNotifier:get_notification(300,  "ScreenLockAC")
IdleNotifier:get_notification(1200,  "ScreenDpmsAC")

IdleNotifier:log("Finished loading idle_config.lua")
