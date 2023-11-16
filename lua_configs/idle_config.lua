print("Loading idle_config.lua")

idle_notifier:get_notification(5, function()
    print("Idle for 10 seconds")
end)

print("Finished loading idle_config.lua")
