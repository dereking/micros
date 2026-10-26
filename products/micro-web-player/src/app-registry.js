// Launcher registry for the Web Player.
//
// The browser has no removable volume, so the Web Player simulates the ESP32's
// SD card with HTTP paths:
//
//   /sdcard/apps/index.json      {"apps":["foo.mbc", "bar.mbc"]}
//   /sdcard/apps/foo.mbc         the external App image
//
// A missing index (404) means "no card" and is ignored. Built-in Apps always
// win an id collision; duplicate external ids and entries that do not fit the
// shell's eight slots are skipped.

export const SD_APPS_INDEX_URL = "/sdcard/apps/index.json";
export const SD_APPS_BASE_URL = "/sdcard/apps/";
export const MAX_LAUNCHER_APPS = 8;

const EXTERNAL_NAME_MAX = 64;

/** True for a bare `*.mbc` file name that cannot escape `/sdcard/apps`. */
export function isValidExternalName(name) {
  if (typeof name !== "string") return false;
  const length = name.length;
  if (length < 5 || length > EXTERNAL_NAME_MAX) return false;
  if (name.startsWith(".") || name.startsWith(" ")) return false;
  if (name.endsWith(" ") || name.endsWith(".")) return false;
  if (!name.toLowerCase().endsWith(".mbc")) return false;
  if (name.includes("/") || name.includes("\\") || name.includes(":")) return false;
  for (const character of name) {
    const code = character.codePointAt(0);
    if (code < 0x20 || code === 0x7f) return false;
  }
  return true;
}


/** External ids are per-App storage namespaces; keep them ASCII identifiers. */
export function isValidExternalId(id) {
  return typeof id === "string" && /^[A-Za-z0-9][A-Za-z0-9_.-]{0,30}$/.test(id);
}

/** Parse the `id|name|icon` string produced by `decode_app_manifest`. */
export function parseManifestString(raw) {
  const first = raw.indexOf("|");
  if (first < 0) return null;
  const second = raw.indexOf("|", first + 1);
  if (second < 0) return null;
  const id = raw.slice(0, first);
  const name = raw.slice(first + 1, second);
  const icon = raw.slice(second + 1);
  if (!id) return null;
  return { id, name, icon };
}

/**
 * Merge built-in and external Apps. Built-ins keep their order; externals are
 * appended in manifest order unless their id (case-insensitive) is taken.
 */
export function mergeAppRegistry(builtin, external, limit = MAX_LAUNCHER_APPS) {
  const merged = [];
  const seen = new Set();

  for (const app of builtin) {
    if (merged.length >= limit) break;
    seen.add(app.id.toLowerCase());
    merged.push({ ...app, external: false });
  }

  for (const app of external) {
    if (merged.length >= limit) break;
    if (!isValidExternalId(app.id)) {
      console.warn(`[apps] ignoring SD App '${app.url}': id '${app.id}' is not a safe identifier`);
      continue;
    }
    const key = app.id.toLowerCase();
    if (key === "shell" || seen.has(key)) {
      console.warn(`[apps] ignoring SD App '${app.url}': id '${app.id}' is already registered or reserved`);
      continue;
    }
    seen.add(key);
    merged.push({ ...app, external: true });
  }

  return merged;
}

/**
 * Discover simulated SD Apps.
 *
 * @param {object} options
 * @param {Array<{id:string,name:string,icon:string,url:string}>} options.builtin
 * @param {(url: string, init?: RequestInit) => Promise<Response>} options.fetchImpl
 * @param {(bytes: Uint8Array) => string} options.decodeManifest
 *        returns the `id|name|icon` string from the Wasm decoder.
 * @param {string} [options.indexUrl]
 * @param {string} [options.baseUrl]
 * @param {number} [options.limit]
 * @returns {Promise<Array<{url:string,id:string,name:string,icon:string}>>}
 */
export async function loadExternalApps({
  builtin,
  fetchImpl,
  decodeManifest,
  indexUrl = SD_APPS_INDEX_URL,
  baseUrl = SD_APPS_BASE_URL,
  limit = MAX_LAUNCHER_APPS,
}) {
  const remaining = Math.max(0, limit - builtin.length);
  if (remaining === 0) return [];

  let response;
  try {
    response = await fetchImpl(indexUrl, { cache: "no-store" });
  } catch {
    return [];
  }
  if (!response.ok) {
    if (response.status !== 404) {
      console.warn(`[apps] cannot read simulated SD index: HTTP ${response.status}`);
    }
    return [];
  }

  let document;
  try {
    document = await response.json();
  } catch {
    console.warn("[apps] simulated SD index is not valid JSON; ignoring it");
    return [];
  }

  const names = Array.isArray(document)
    ? document
    : Array.isArray(document?.apps)
      ? document.apps
      : [];
  const seen = new Set(builtin.map((app) => app.id.toLowerCase()));
  const apps = [];

  for (const name of names.slice().sort()) {
    if (apps.length >= remaining) break;
    if (!isValidExternalName(name)) {
      console.warn(`[apps] ignoring invalid SD App name '${name}'`);
      continue;
    }
    const url = `${baseUrl}${encodeURIComponent(name)}`;
    let bytes;
    try {
      const file = await fetchImpl(url);
      if (!file.ok) {
        console.warn(`[apps] cannot read SD App ${url}: HTTP ${file.status}`);
        continue;
      }
      bytes = new Uint8Array(await file.arrayBuffer());
    } catch (error) {
      console.warn(`[apps] cannot read SD App ${url}: ${error}`);
      continue;
    }

    let manifest;
    try {
      manifest = parseManifestString(decodeManifest(bytes));
    } catch (error) {
      console.warn(`[apps] ignoring SD App ${url}: ${error}`);
      continue;
    }
    if (!manifest) {
      console.warn(`[apps] ignoring SD App ${url}: manifest has no id`);
      continue;
    }
    if (!isValidExternalId(manifest.id)) {
      console.warn(`[apps] ignoring SD App ${url}: id '${manifest.id}' is not a safe identifier`);
      continue;
    }
    const key = manifest.id.toLowerCase();
    if (key === "shell" || seen.has(key)) {
      console.warn(`[apps] ignoring SD App ${url}: id '${manifest.id}' is already registered or reserved`);
      continue;
    }
    seen.add(key);
    apps.push({ url, ...manifest });
  }

  return apps;
}
