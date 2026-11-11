// Suite calls to the page's own server. From core ADR 0030 (freemapper-web), taken from
// freemapper-courseplan's editor. Served at {token}/suite/api.js.
// Paths are relative, so the token in the page's path carries along.
// Every call answers { ok, status, data }; data is the parsed JSON, or the plain text of an error answer.
// A POST sends a content type a cross-site form cannot send (application/json or application/octet-stream).
// It names the revision the page last saw in X-Freemapper-Revision, so the server can refuse a stale write
// with 409; a change the app refuses gets 422.

export const REVISION_HEADER = "X-Freemapper-Revision";

async function answer(response) {
  const text = await response.text();
  let data = text;
  try {
    data = JSON.parse(text);
  } catch {
    // An error answer is plain text.
  }
  return { ok: response.ok, status: response.status, data };
}

// While the page is read-only (for example during a print preview), nothing may change a file: a write is
// refused here with 423, whatever asked for it, and no request is sent.
let readOnly = false;
export function setReadOnly(value) {
  readOnly = value;
}
const REFUSED = { ok: false, status: 423, data: "read-only" };

export async function get(path) {
  return answer(await fetch(path, { cache: "no-store" }));
}

export async function post(path, body = {}, revision = null) {
  if (readOnly) return REFUSED;
  const headers = { "Content-Type": "application/json" };
  if (revision) headers[REVISION_HEADER] = revision;
  return answer(await fetch(path, { method: "POST", headers, body: JSON.stringify(body) }));
}

// A body that is not JSON, sent as application/octet-stream; the answer is parsed like the others.
export async function postBytes(path, bytes, revision = null) {
  if (readOnly) return REFUSED;
  const headers = { "Content-Type": "application/octet-stream" };
  if (revision) headers[REVISION_HEADER] = revision;
  return answer(await fetch(path, { method: "POST", headers, body: bytes }));
}

// A file to hand to the person: { ok, status, blob, headers } on success, or the parsed error answer.
export async function getFile(path) {
  const response = await fetch(path, { cache: "no-store" });
  if (!response.ok) return answer(response);
  return { ok: true, status: response.status, blob: await response.blob(), headers: response.headers };
}
