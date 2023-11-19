print("Loading idle_config.lua")

function ScreenLockBattery(event)
  if event == "idled" then
    print("Locking Screen")
    IdleNotifier:run_once("swaylock -f")
  end
end

function ScreenDpmsBattery(event)
  if event == "idled" then
    print("Turning screen off")
    IdleNotifier:run_once("swaymsg output '*' dpms off")
  elseif event == "resumed" then
    print("Turning screen on")
    IdleNotifier:run_once("swaymsg output '*' dpms on")
  end
end

function ScreenLockAC(event)
  if event == "idled" then
    print("Locking Screen")
    IdleNotifier:run_once("swaylock -f")
  end
end

function ScreenDpmsAC(event)
  if event == "idled" then
    print("Turning screen off")
    IdleNotifier:run_once("swaymsg output '*' dpms off")
  elseif event == "resumed" then
    print("Turning screen on")
    IdleNotifier:run_once("swaymsg output '*' dpms on")
  end
end


IdleNotifier:get_notification(30,  "ScreenLockBattery")
IdleNotifier:get_notification(10,  "ScreenDpmsAC")

print("Finished loading idle_config.lua")
