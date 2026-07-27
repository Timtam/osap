-- Recognizes on-screen text in a region (Windows.Media.Ocr). This is the last
-- building block the ReaHotkey overlays need (e.g. reading a plugin's preset name).

local r = host.ocr.recognize({ region = { 0, 0, 900, 350 } })

host.log.info(string.format("OCR recognized %d words.", #r.words))
if r.text ~= "" then
  host.log.info("Text: " .. r.text)
else
  host.log.info("Text: <none recognized in region>")
end

host.speech.output(string.format("Recognized %d words.", #r.words), { interrupt = true })
