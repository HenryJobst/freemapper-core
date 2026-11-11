// Suite texts: every visible word comes from the app's Fluent catalog through GET api/texts, which answers
// { lang, texts }. From core ADR 0030 (freemapper-web), taken from freemapper-courseplan's editor. Served at
// {token}/suite/texts.js.
// An element names its text by id: data-text (content), data-label (aria-label), data-title (title),
// data-placeholder (placeholder). The page title is the data-text of <title>. An unknown id shows the id.
// Patterns may hold simple placeholders { $name }; t(id, args) fills them in, tn(id, n, args) picks
// <id>-one or <id>-other by Intl.PluralRules. Numbers are formatted for the page's language.

import { get } from "./api.js";

let catalog = {};
let lang = "en";

// The language the server chose for the page ("de" or "en").
export function language() {
  return lang;
}

const PLACEHOLDER = /\{\s*\$([A-Za-z][A-Za-z0-9_-]*)\s*\}/g;

function shown(value) {
  return typeof value === "number" || typeof value === "bigint" ? value.toLocaleString(lang) : String(value);
}

export function t(id, args = {}) {
  const pattern = Object.hasOwn(catalog, id) && typeof catalog[id] === "string" ? catalog[id] : id;
  return pattern.replace(PLACEHOLDER, (whole, name) => (Object.hasOwn(args, name) ? shown(args[name]) : whole));
}

export function tn(id, n, args = {}) {
  const form = new Intl.PluralRules(lang).select(n) === "one" ? "one" : "other";
  return t(`${id}-${form}`, { ...args, n });
}

export async function loadTexts() {
  const answer = await get("api/texts");
  if (!answer.ok) throw new Error(`api/texts: ${answer.status}`);
  catalog = answer.data.texts ?? {};
  lang = answer.data.lang ?? "en";
  document.documentElement.lang = lang;
  apply(document);
}

const ATTRIBUTES = [
  ["data-text", (el, text) => { el.textContent = text; }],
  ["data-label", (el, text) => el.setAttribute("aria-label", text)],
  ["data-title", (el, text) => el.setAttribute("title", text)],
  ["data-placeholder", (el, text) => el.setAttribute("placeholder", text)],
];

// Applies the texts below root; call it again for elements the page adds later.
export function apply(root) {
  for (const [attribute, set] of ATTRIBUTES) {
    for (const el of root.querySelectorAll(`[${attribute}]`)) {
      set(el, t(el.getAttribute(attribute)));
    }
  }
}
