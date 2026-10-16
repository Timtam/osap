// page-check's page script (crates/host/src/backend/macos/page_check.rs). It answers the check's
// commands; every message both ways is a plain string, so the check reads it without a JSON parser.
"use strict";

const field = document.getElementById("field");
const button = document.getElementById("button");
let clicks = 0;
let violations = 0;

document.addEventListener("securitypolicyviolation", () => {
  violations += 1;
});
button.addEventListener("click", () => {
  clicks += 1;
  platform.post("clicked " + clicks);
});
// Every press, where it is in the page: a posted click is checked by the place it lands at.
document.addEventListener("mousedown", (event) => {
  platform.post("down " + Math.round(event.clientX) + " " + Math.round(event.clientY) + " " + event.button);
});
document.addEventListener("copy", () => platform.post("copied"));
field.addEventListener("input", () => platform.post("value " + field.value));

const commands = {
  echo: (n) => platform.post("echo " + n),
  "fetch-own": () => {
    fetch("data.json")
      .then((r) => (r.ok ? r.json() : Promise.reject(new Error("status " + r.status))))
      .then((data) => platform.post("fetch-own ok " + data.from), (err) => platform.post("fetch-own failed " + err.message));
  },
  "fetch-outside": () => {
    fetch("https://example.com/").then(
      (r) => platform.post("fetch-outside answered " + r.status),
      () => platform.post("fetch-outside blocked, policy violations so far " + violations),
    );
  },
  // A file of the page's own, which the policy lets it fetch: only a rule list can block it — the
  // one page-check builds this page with to try the rule list on its own.
  "fetch-rules-only": () => {
    fetch("rules-only.json").then(
      (r) => platform.post("fetch-rules-only answered " + r.status),
      () => platform.post("fetch-rules-only blocked, policy violations so far " + violations),
    );
  },
  navigate: () => {
    location.href = "https://example.com/";
  },
  close: () => {
    window.close();
    platform.post("close called");
  },
  "focus-field": () => {
    field.focus();
    platform.post("focused " + (document.activeElement === field ? "field" : "other"));
  },
  // Where the button is in the page, in CSS pixels, which are points at the web view's own zoom.
  "button-rect": () => {
    const r = button.getBoundingClientRect();
    platform.post("button-rect " + [r.left, r.top, r.width, r.height].join(" "));
  },
  "focus-button": () => {
    button.focus();
    platform.post("focused " + (document.activeElement === button ? "button" : "other"));
  },
};

platform.onmessage = (text) => {
  const [name, arg] = String(text).split(" ");
  const run = commands[name];
  if (run) run(arg);
  else platform.post("unknown " + name);
};

platform.post(
  [
    "hello",
    "isSecureContext=" + window.isSecureContext,
    "navigator.clipboard=" + typeof navigator.clipboard,
    "RTCPeerConnection=" + typeof window.RTCPeerConnection,
  ].join(" "),
);
