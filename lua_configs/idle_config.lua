print("Loading idle_config.lua")


function NotifyToast()
  print("idle notification text")
end

idle_notifier:get_notification(5,  "NotifyToast")
-- idle_notifier:get_notification(5)
-- callback functions don't work because mlua::Functions cannot be passed around threads
-- idle_notifier:get_notification(5, function()
--     print("Idle for 10 seconds")
-- end)

print("Finished loading idle_config.lua")
