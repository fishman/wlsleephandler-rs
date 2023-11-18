print("Loading idle_config.lua")


function SleepHandler(event)
  if event == "idled" then
    print("system goes to idle")
    IdleNotifier:run_once("swaylock -f")
  elseif event == "resumed" then
    print("system resuming")
  end
end

IdleNotifier:get_notification(10,  "SleepHandler")

print("Finished loading idle_config.lua")
