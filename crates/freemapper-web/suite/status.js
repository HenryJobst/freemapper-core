// Suite status: the message area of the status bar and the banner. From core ADR 0030 (freemapper-web),
// taken from freemapper-courseplan's editor. Served at {token}/suite/status.js.
// The page provides two elements with fixed ids:
//   #status-message  the message area, <span id="status-message" role="status" aria-live="polite">
//   #banner          the banner region, <div id="banner" role="alert">
// The dismiss button of the banner shows the text "suite-banner-dismiss" from the app's catalog.
// The app adds its own status fields (file, scale, cursor and so on) in its own module.

import { t } from "./texts.js";

// Shows a message by its text id; with raw = true the text is shown as it is (server answers come localized).
export function say(idOrText, raw = false) {
  document.getElementById("status-message").textContent = raw ? idOrText : t(idOrText);
}

// A refusal or a conflict stays until it is dismissed or cleared. The region is role="alert", so a screen
// reader reads the text when it appears.
export function showBanner(text) {
  const box = document.createElement("div");
  box.className = "banner-box";
  const words = document.createElement("span");
  words.textContent = text;
  const close = document.createElement("button");
  close.type = "button";
  close.textContent = t("suite-banner-dismiss");
  close.addEventListener("click", clearBanner);
  box.append(words, close);
  document.getElementById("banner").replaceChildren(box);
}

export function clearBanner() {
  document.getElementById("banner").replaceChildren();
}
