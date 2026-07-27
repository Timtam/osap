-- Demonstrates host.input. Non-destructive: only moves the mouse cursor and
-- restores it. (click/drag/scroll/send/text act on the focused window, so use
-- them deliberately, not in a demo.)

local before = host.input.cursorPos()
host.log.info(string.format("Cursor was at %d,%d", before.x, before.y))

local size = host.screen.size()
host.input.move(math.floor(size.w / 2), math.floor(size.h / 2))
host.log.info("Moved cursor to screen center.")

host.input.move(before.x, before.y)
host.log.info("Restored cursor position.")

host.speech.output("Input demo: moved the cursor to center and back.", { interrupt = true })
