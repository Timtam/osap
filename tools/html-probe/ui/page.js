// The probe's page: it reports to the module every key event, every focus change and every
// button press, and shows what the module posts. Data only ever goes into the page as text
// (`textContent`), never as HTML, and every action is a real button handled on "click" — which
// is also what NVDA's browse mode sends when Enter or Space is pressed on a button.
"use strict";

const send = (value) => {
  if (window.platform) window.platform.post(value);
};

// What the host's policy does, measured by the page itself; the answers are in the log. Each
// refusal of the Content-Security-Policy header is reported as it happens — which also says the
// header is enforced at all.
document.addEventListener("securitypolicyviolation", (event) => {
  send({ kind: "policy", directive: event.effectiveDirective || event.violatedDirective, blocked: event.blockedURI || "inline" });
});

// An element in a few words, for the log: its tag, its name and its id.
const describe = (el) => {
  if (!el || el === document.body || el === document.documentElement) return "the document";
  const name = el.getAttribute("aria-label")
    || (el.labels && el.labels[0] && el.labels[0].textContent)
    || (el.tagName === "BUTTON" ? el.textContent : "")
    || "";
  const id = el.id ? "#" + el.id : "";
  return el.tagName.toLowerCase() + id + (name ? " '" + name.trim() + "'" : "");
};

const key = (type) => (event) => {
  send({
    kind: "key",
    type,
    key: event.key,
    code: event.code,
    repeat: event.repeat,
    ctrl: event.ctrlKey,
    alt: event.altKey,
    shift: event.shiftKey,
    meta: event.metaKey,
    target: describe(event.target),
  });
};
// In the capture phase, so a key is reported before anything on the page can stop it.
document.addEventListener("keydown", key("keydown"), true);
document.addEventListener("keyup", key("keyup"), true);

window.addEventListener("focus", () => send({ kind: "focus", target: describe(document.activeElement) }));
window.addEventListener("blur", () => send({ kind: "blur" }));
document.addEventListener("focusin", (event) => send({ kind: "focusin", target: describe(event.target) }));
document.addEventListener("visibilitychange", () => send({ kind: "visibility", state: document.visibilityState }));

// What was released in the page since the last button was activated: a key going up, or a
// pointer. A person's second press of a button releases something before it activates again —
// a key's up, or, for NVDA's activation in browse mode, the pointerup and mouseup Chromium sends
// ahead of an accessibility click — so a second activation with nothing released since is one
// press arriving twice (test W5).
let releases = [];
const released = (what) => {
  if (releases.length < 6) releases.push(what);
};
document.addEventListener("keyup", (event) => released("keyup " + (event.key === " " ? "Space" : event.key)), true);
document.addEventListener("pointerup", () => released("pointerup"), true);
document.addEventListener("mouseup", () => released("mouseup"), true);

// The page's own count of each button's clicks, sent with every press: the module counts the
// messages it receives, and the two numbers disagree when one message arrives twice (test W5).
const counts = {};
let previous = null;
document.addEventListener("click", (event) => {
  const button = event.target.closest("[data-action]");
  if (!button) return;
  const action = button.dataset.action;
  counts[action] = (counts[action] || 0) + 1;
  send({
    kind: "action",
    action,
    count: counts[action],
    detail: event.detail,
    previous,
    released: releases.length > 0,
    releases,
  });
  previous = action;
  releases = [];
});

// W10: "Read the plug-in's elements" and "Try it". Each sends what the three selects hold now —
// the way, the target and how many times — so the module need keep no state of the page's. The
// target select is refilled from the module's element list (below).
const w10Select = (id) => {
  const el = document.getElementById(id);
  return el ? el.value : null;
};
document.addEventListener("click", (event) => {
  const button = event.target.closest("[data-w10]");
  if (!button) return;
  send({
    kind: "w10",
    do: button.dataset.w10,
    way: w10Select("w10-way"),
    target: w10Select("w10-target"),
    repeat: w10Select("w10-repeat"),
  });
});

document.getElementById("search").addEventListener("change", (event) => {
  send({ kind: "search", value: event.target.value });
});
document.getElementById("choice").addEventListener("change", (event) => {
  send({ kind: "select", value: event.target.value });
});

const live = document.getElementById("live");
const heading = document.getElementById("heading");
const switchButton = document.getElementById("switch");
const w10Part = document.getElementById("w10");
if (window.platform) {
  window.platform.onmessage = (message) => {
    if (!message || typeof message !== "object") return;
    if (message.kind === "live") live.textContent = String(message.text);
    else if (message.kind === "heading") heading.textContent = String(message.text);
    // Which window this page is in, said once it has loaded: its switch button names the other.
    // And whether W10 is offered on this platform: Windows' alone. Its part is hidden until this
    // says it is offered, so a Mac's page never shows it, not even before this message comes, and
    // the screen reader there never reads it.
    else if (message.kind === "window") {
      switchButton.textContent = message.window === "app" ? "Show as a tool window" : "Show as an application window";
      if (w10Part) w10Part.hidden = message.w10 !== true;
    }
    // W10: the plug-in's elements, for the Target select — the remembered point first, then each
    // element by name and type. Rebuilt from the message, never from HTML the module sent.
    else if (message.kind === "w10elements") {
      const sel = document.getElementById("w10-target");
      if (!sel) return;
      sel.textContent = "";
      const point = document.createElement("option");
      point.value = "point";
      point.textContent = message.target ? "the remembered pointer point (" + String(message.target) + ")" : "the remembered pointer point";
      sel.appendChild(point);
      for (const item of Array.isArray(message.elements) ? message.elements : []) {
        const option = document.createElement("option");
        option.value = String(item.value);
        option.textContent = String(item.label);
        sel.appendChild(option);
      }
    }
  };
} else {
  // Without the host's script nothing reaches the module: said where NVDA reads it and a
  // screenshot shows it, rather than every report going nowhere.
  live.textContent = "This page cannot reach the module.";
}

// Three things the policy decides, tried once: code made from a string must not run; the page's
// own file is fetched; and a name that exists nowhere (`.invalid` is reserved, so nothing answers
// it even where the policy did not hold) must be refused before any request.
let evalRuns = "refused";
try {
  new Function("return 1")();
  evalRuns = "RAN";
} catch (err) {
  evalRuns = "refused";
}
const tryFetch = (what, url) => fetch(url).then(
  (response) => send({ kind: "check", what, status: "answered " + response.status }),
  (err) => send({ kind: "check", what, status: "refused (" + String(err) + ")" }),
);
tryFetch("a fetch of its own page.css", "page.css");
tryFetch("a fetch of https://example.invalid/", "https://example.invalid/probe");

send({
  kind: "loaded",
  hasFocus: document.hasFocus(),
  evalRuns,
  rtc: typeof RTCPeerConnection,
  clipboardWrite: Boolean(navigator.clipboard && navigator.clipboard.writeText),
});
