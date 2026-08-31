-- Reading this window's accessibility tree, for whichever overlay is asking.
--
-- One crossing of the process boundary, timed, and nothing above it: the helpers that find a
-- named element or the value beside it are per-overlay and live with the panel that needs
-- them. This is only the read.

-- ── The accessibility tree, read once and indexed ────────────────────────────
--
-- Read fresh, every time, and no cache anywhere in this module.
--
-- It did have one, and the story is worth keeping because the lesson is not about this
-- module. A walk of this 53-element window cost **211 ms**, every Tab paid it before a word
-- was spoken, and the module grew a snapshot, a staleness rule and an exception for the two
-- read-outs that change on their own. All of that was compensation for a defect one layer
-- down: the backend was asking the target process for a name, a class, a type and a
-- rectangle one property at a time. Asking for them all at once — a UIA cache request —
-- brought the same walk to about **30 ms**, and every one of those compensations became a
-- complication with nothing left to justify it.
--
-- So they are gone, and what is left is simpler AND more correct: nothing here can be stale,
-- including the positions, which the cached version would have kept believing across a
-- window resize the overlay did not notice.
--
-- The timing stays. It is what turned "there is a noticeable delay" into a number, and it is
-- what will say so again if this module ever meets a window with ten times the elements.
local slowestTree = 0
local function dumpFor(overlay)
  local o = overlay:origin()
  if not (o and o.id) then return nil end
  local started = host.now()
  local dump = host.uia.rawDump(o.id)
  local took = host.now() - started
  if took > slowestTree then
    slowestTree = took
    host.log.info(("[on-ear] reading the accessibility tree took %d ms (%d elements)")
      :format(took, #(dump or {})))
  end
  return dump
end

-- Memoised per EPOCH, which is not the cache this file used to have and argued its way out
-- of. That one had a time-to-live, so it could hand back a picture of the window from before
-- something the user had just done. An epoch is one pump of the host: every read inside it
-- happens between two of the user's actions, so a shared answer cannot be stale, and the
-- second control to ask in the same breath asks for free.
--
-- It matters more now than it did. A single announcement in the chooser panel below wants the
-- selected model, its description and the row that is highlighted — three questions of the
-- same tree, and without this, three crossings of a process boundary to answer them.

return { dumpFor = dumpFor }
