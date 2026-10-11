// The click helper's page. Every action is a real button handled on "click" — which is also
// what NVDA's browse mode sends when Enter or Space is pressed on a button — and every text the
// module sends goes into the page as text (`textContent`), never as HTML.
"use strict";

const status = document.getElementById("status");
const target = document.getElementById("target");

if (window.platform) {
  // What goes wrong in the page, for the module's log: a page has no console to look in. From
  // here on only — a script that fails to load or to parse reports nothing, this one included.
  window.addEventListener("error", (event) => {
    window.platform.post({ kind: "problem", text: `${event.message} (${event.filename}:${event.lineno})` });
  });
  document.addEventListener("securitypolicyviolation", (event) => {
    window.platform.post({ kind: "problem", text: `the policy refused ${event.blockedURI || "inline code"} (${event.effectiveDirective})` });
  });

  document.addEventListener("click", (event) => {
    const button = event.target.closest("button[data-action]");
    if (button) window.platform.post({ action: button.dataset.action });
  });
  // A status comes while the page is in front: a change of the live region, the focus left where
  // it is. A round trip's result comes while the target window has the keyboard, just before the
  // page is brought back: the status line, and the keyboard focus on it, so that the page comes
  // back on the result. A page coming to the front is announced with the element that has the
  // focus, and that announcement cuts off a sentence said just before it. (The status line's
  // tabindex="-1" lets a script focus it and keeps it out of Tab's way.)
  window.platform.onmessage = (message) => {
    if (!message || typeof message !== "object") return;
    if (message.kind === "status") {
      status.textContent = String(message.text);
    } else if (message.kind === "result") {
      status.textContent = String(message.text);
      status.focus();
    } else if (message.kind === "target") {
      target.textContent = String(message.title);
    }
  };
} else {
  // Opened in a browser, to work on how it looks and reads: there is no module to talk to.
  status.textContent = "This page is not running in the automation platform.";
}
