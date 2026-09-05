-- Does a line of speech reach the voice it was told to reach?
--
-- Not for a tester — for CI. The macOS speech path was rewritten from scratch (`avspeech.rs`,
-- replacing `tts`) and has never run anywhere but a build machine, and speech is the one new
-- subsystem a CI runner can actually exercise: every accessibility call needs a permission a
-- runner does not have, and speaking needs none.
--
-- What compiling already proves is that the calls exist. What it cannot prove is the WIRING —
-- which of several possible voices a line is handed to, whether choosing one takes effect,
-- whether the list is even populated by the time anybody asks. That half is where the
-- mistakes are, and it is the half a blind tester would otherwise find first, as item one of
-- his session.
--
-- Everything it prints is prefixed, so a job can assert on it rather than on a person reading
-- a log.

local MARK = "SPEECH PROBE:"

-- How long to wait for a voice to become choosable, and how often to look.
--
-- Asking once was the first version and it was wrong on the platform it was written on: the
-- list is built lazily and off the calling thread, deliberately, so that a session which says
-- nothing pays nothing for speech. Measured while writing this — macOS read 191 voices in
-- 282 ms on a CI runner, while Windows offered none at 1.5 s and opened its first voice at
-- about 5 s. A single early question would have reported "no voice on this machine" and been
-- believed.
--
-- So this waits, and reports how long it waited, which is a number worth having on its own:
-- it is how long after start-up a module can actually choose a voice.
local DEADLINE_MS = 12000
local STEP_MS = 500

local function report(fmt, ...)
  if select("#", ...) > 0 then
    host.log.info(MARK .. " " .. string.format(fmt, ...))
  else
    host.log.info(MARK .. " " .. fmt)
  end
end

-- A screen reader is not what is being tested: none runs on a build machine, and the point is
-- the engine this platform provides itself.
local function firstPlainVoice(engines)
  for _, e in ipairs(engines) do
    if not e.screenReader and e.available then
      return e
    end
  end
  return nil
end

local function examine(chosen, engines)
  report("%d engine(s) offered", #engines)
  for _, e in ipairs(engines) do
    report(
      "  engine id='%s' name='%s' screenReader=%s available=%s",
      tostring(e.id), tostring(e.name), tostring(e.screenReader), tostring(e.available)
    )
  end

  local asked, accepted = pcall(function()
    return host.speech.use(chosen.id)
  end)
  report(
    "chose '%s' (%s): use() %s",
    chosen.name, chosen.id,
    asked and (accepted and "accepted" or "REFUSED") or "THREW"
  )

  -- The say itself. A runner has no audio device, so nothing is heard and nothing should be:
  -- what is being asked is whether handing a line over survives the trip at all. A crash here
  -- would be an unrecognised selector, which on this objc2 version aborts the process rather
  -- than returning nothing — the exact failure this file exists to catch before a tester does.
  local said = pcall(function()
    host.speech.output("Speech probe: one line, said on purpose.", { interrupt = true })
  end)
  report("spoke through '%s': output() %s", chosen.name, said and "returned" or "THREW")

  -- `engine()` answers what was CHOSEN, as a string id — not what is speaking. That is the
  -- promise the API documents, and this is the one place that checks it is kept.
  local read, now = pcall(function()
    return host.speech.engine()
  end)
  if read then
    report(
      "engine() reports %s — %s",
      tostring(now),
      now == chosen.id and "the choice stuck" or "WHICH IS NOT WHAT WAS CHOSEN"
    )
  else
    report("engine() threw: %s", tostring(now))
  end

  -- And one more line, a second and a half later, which is the whole point of this file.
  --
  -- `output` hands the utterance to a channel and returns; the AVFoundation call happens
  -- afterwards on the worker thread. So "done" was written BEFORE the selector that could
  -- abort the process was ever sent, and a job asserting on "done" would have passed on a
  -- process that died a moment later — while the one named failure this exists to catch is
  -- exactly that: "a selector that does not exist aborts the process instead of returning
  -- nothing". A line that can only be written by a process that is still running, after the
  -- call that could stop it, is the assertion the job believes it is making.
  host.timer.after(1500, function()
    report("still alive after speaking")
  end)
end

local waited = 0
local function attempt()
  -- Counted at the top, so the number reported is the time actually WAITED before the look
  -- that succeeded. Counting it after would report the first look as "0 ms" when half a
  -- second had passed. It is an upper bound at this granularity, and says so.
  waited = waited + STEP_MS
  local listed, engines = pcall(function()
    return host.speech.engines()
  end)
  if not listed or type(engines) ~= "table" then
    report("engines() failed: %s", tostring(engines))
    report("done")
    return
  end
  local chosen = firstPlainVoice(engines)
  if chosen then
    report("a voice was choosable within %d ms of load", waited)
    examine(chosen, engines)
    report("done")
    return
  end
  if waited >= DEADLINE_MS then
    report(
      "no plain voice was offered within %d ms (%d engine(s) listed) — nothing here can speak",
      DEADLINE_MS, #engines
    )
    report("done")
    return
  end
  host.timer.after(STEP_MS, attempt)
end

host.timer.after(STEP_MS, attempt)
report("started; waiting up to %d ms for a voice to become choosable", DEADLINE_MS)
