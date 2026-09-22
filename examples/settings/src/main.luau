-- Settings demo: declare a setting of each kind, then read it. `define` both
-- declares (pinning type + constraints + a label for the settings GUI) and
-- returns the current value (a persisted value wins over the default).

local imageSearch = host.settings.define("imageSearch", false, { label = "Image search" })
local rate = host.settings.define("speechRate", 50, { label = "Speech rate", min = 0, max = 100 })
-- BCP-47 language tags rather than Tesseract's three-letter codes, which neither platform's
-- OCR engine knows. (This demo only reads the setting back; see docs/api/ocr.md for what
-- `lang` takes when it reaches host.ocr.)
local lang = host.settings.define("ocrLanguage", "en", {
  label = "OCR language",
  oneOf = { "en", "de", "fr" },
})

host.log.info(
  "loaded: imageSearch="
    .. tostring(imageSearch)
    .. " speechRate="
    .. tostring(rate)
    .. " ocrLanguage="
    .. lang
)

-- React to changes (fired by host.settings.set and the settings GUI).
host.settings.onChange("speechRate", function(new, old)
  host.log.info("speechRate changed " .. tostring(old) .. " -> " .. tostring(new))
end)

-- Stay resident + give an interactive check: speak the current values. After
-- changing them in the manager's "Settings…" dialog, this reflects the new ones.
host.hotkey.register("Ctrl+Alt+S", function()
  host.speech.output(
    "Image search "
      .. (host.settings.get("imageSearch") and "on" or "off")
      .. ", speech rate "
      .. host.settings.get("speechRate")
      .. ", language "
      .. host.settings.get("ocrLanguage"),
    { interrupt = true }
  )
end)
