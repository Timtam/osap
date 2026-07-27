-- The platform's first module: speaks through the host API, with no DLL binding.

host.log.info("Hello World module loaded.")

local manifest = host.resource.read("module.toml")
host.log.info("My manifest is " .. #manifest .. " bytes (read via host.resource.read).")

host.speech.output("Hello! The platform's first module is speaking.", { interrupt = true })

host.log.info("Path to my manifest: " .. host.path("module.toml"))
