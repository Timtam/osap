-- What every part of this module needs to know about the application itself.
--
-- The window it lives in, the arbiter slot its panels compete on, and the one element whose
-- presence proves the accessibility tree is answering at all. None of those belong to any one
-- panel, and the last of them was written down twice once already — two spellings of one
-- question let a gate and the check beside it disagree about whether a panel was still open.

-- The window. Identified by the executable rather than the window class: the class is
-- `JUCE_1a05739a091`, and that hexadecimal tail is generated per build of the JUCE runtime —
-- matching on it would work today and break at the next update, silently, in the way that is
-- hardest to diagnose from the outside. The title carries a middle dot (ARC ON·EAR), so both
-- matches deliberately stop before it and stay ASCII.
local on_ear = {
  app = { exe = { contains = "ARC ON" } },
  windows = { class = { contains = "JUCE_" } },
}

-- One arbiter slot for this window. The main overlay and the chooser panels all match it,
-- and the most specific one that is actually on screen wins.
local ON_EAR_SLOT = "com.platform.ik-on-ear/window"

-- How this application's menus are seen, for every overlay here (see the overlay runtime's
-- Menus section). A JUCE combo box opens its list as a popup WINDOW of its own, which is what
-- newWindow sees; nativePopup costs one flag read and catches any menu the system draws.
-- Not the accessibility test: in the only session that opened these menus (2026-08-31, four
-- presses) it ran on every tick after each press and saw none of them — the popup is a window
-- of its own, outside the tree it walks. Whether newWindow sees them is a live check still to
-- be made: that session ran before it existed.
local O = host.require("com.platform.overlay")
local MENUS = { O.menuTests.nativePopup, O.menuTests.newWindow }

return {
  window = on_ear,
  SLOT = ON_EAR_SLOT,
  MENUS = MENUS,
  -- Something this window's tree carries whenever the tree works at all. Every panel here uses
  -- it to tell "the thing I am looking for is gone" apart from "the application has stopped
  -- answering", which it does whenever a caret is in one of its text fields.
  SENTINEL = "Battery Level",
}
