-- Recognizes on-screen text in a region (Windows.Media.Ocr). This is the last
-- building block the ReaHotkey overlays need (e.g. reading a plugin's preset name).
--
-- With a callback: the read is photographed and recognised off the event loop, and the
-- reading comes on a later turn of the loop. A module's top level cannot wait for it
-- (see "Where it waits" in docs/api/ocr.md).

host.ocr.recognize({ region = { 0, 0, 900, 350 } }, function(r)
  if r.status == "failed" then
    -- The capture failed, no language is available, or the recogniser is not answering.
    host.log.info("OCR failed: " .. r.error)
    host.speech.output("The text could not be read.", { interrupt = true })
    return
  end

  host.log.info(string.format("OCR recognized %d words.", #r.words))
  if r.text ~= "" then
    host.log.info("Text: " .. r.text)
  else
    host.log.info("Text: <none recognized in region>")
  end

  host.speech.output(string.format("Recognized %d words.", #r.words), { interrupt = true })
end)
