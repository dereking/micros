import assert from "node:assert/strict";
import test from "node:test";

import {
  MAX_LAUNCHER_APPS,
  isValidExternalId,
  isValidExternalName,
  loadExternalApps,
  mergeAppRegistry,
  parseManifestString,
} from "../../products/micro-web-player/src/app-registry.js";

const builtin = [
  { id: "counter", name: "Counter", icon: "plus", url: "/apps/counter.mbc" },
  { id: "settings", name: "Settings", icon: "settings", url: "/apps/settings.mbc" },
];

function responseJson(value, status = 200) {
  return {
    ok: status >= 200 && status < 300,
    status,
    json: async () => value,
  };
}

function responseBytes(bytes, status = 200) {
  return {
    ok: status >= 200 && status < 300,
    status,
    arrayBuffer: async () => bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength),
  };
}

/** Fake decoder keyed by the bytes we serve for each URL. */
function decodeManifest(bytes) {
  const key = new TextDecoder().decode(bytes);
  const manifests = {
    "counter.mbc": "counter|Counter|plus",
    "extra.mbc": "extra|Extra|folder",
    "second.mbc": "second|Second|folder",
    "unsafe.mbc": "../escape|Unsafe|folder",
  };
  return manifests[key] ?? "broken";
}

test("external names must be bare .mbc file names", () => {
  assert.equal(isValidExternalName("notes.mbc"), true);
  assert.equal(isValidExternalName("My App.MBC"), true);
  for (const invalid of ["../notes.mbc", "/notes.mbc", "a/b.mbc", "a\\b.mbc", "notes.txt", ".mbc", "bad.mbc "]) {
    assert.equal(isValidExternalName(invalid), false, invalid);
  }
});

test("manifest strings split id, name and icon", () => {
  assert.deepEqual(parseManifestString("counter|Counter|plus"), {
    id: "counter",
    name: "Counter",
    icon: "plus",
  });
  assert.equal(parseManifestString("no-separator"), null);
});

test("built-ins win id collisions and externals append in order", () => {
  const merged = mergeAppRegistry(builtin, [
    { id: "Extra", name: "Extra", icon: "folder", url: "/sdcard/apps/extra.mbc" },
    { id: "COUNTER", name: "Fake Counter", icon: "plus", url: "/sdcard/apps/counter.mbc" },
    { id: "shell", name: "Fake Home", icon: "apps", url: "/sdcard/apps/shell.mbc" },
    { id: "../escape", name: "Unsafe", icon: "folder", url: "/sdcard/apps/escape.mbc" },
  ]);
  assert.deepEqual(
    merged.map((app) => app.id),
    ["counter", "settings", "Extra"],
  );
  assert.equal(merged[2].external, true);
  assert.equal(merged[0].external, false);
  assert.equal(isValidExternalId("../escape"), false);
});

test("the merged launcher is capped at eight slots", () => {
  const external = Array.from({ length: MAX_LAUNCHER_APPS + 3 }, (_, index) => ({
    id: `extra${index}`,
    name: `Extra ${index}`,
    icon: "folder",
    url: `/sdcard/apps/extra${index}.mbc`,
  }));
  assert.equal(mergeAppRegistry(builtin, external).length, MAX_LAUNCHER_APPS);
});

test("a missing simulated card is ignored", async () => {
  const apps = await loadExternalApps({
    builtin,
    fetchImpl: async () => responseJson({}, 404),
    decodeManifest,
  });
  assert.deepEqual(apps, []);
});

test("simulated SD apps are decoded, sorted and de-duplicated", async () => {
  const requests = [];
  const served = new Map([
    ["/sdcard/apps/index.json", { json: { apps: ["second.mbc", "counter.mbc", "extra.mbc", "unsafe.mbc", "../bad.mbc"] } }],
    ["/sdcard/apps/second.mbc", { bytes: new TextEncoder().encode("second.mbc") }],
    ["/sdcard/apps/counter.mbc", { bytes: new TextEncoder().encode("counter.mbc") }],
    ["/sdcard/apps/extra.mbc", { bytes: new TextEncoder().encode("extra.mbc") }],
    ["/sdcard/apps/unsafe.mbc", { bytes: new TextEncoder().encode("unsafe.mbc") }],
  ]);
  const fetchImpl = async (url) => {
    requests.push(url);
    const entry = served.get(url);
    if (!entry) return responseJson({}, 404);
    return entry.json ? responseJson(entry.json) : responseBytes(entry.bytes);
  };

  const external = await loadExternalApps({ builtin, fetchImpl, decodeManifest });
  assert.deepEqual(
    external.map((app) => app.id),
    ["extra", "second"],
    "sorted by file name; duplicate/new-collision ids are skipped",
  );
  assert.ok(!requests.includes("/sdcard/apps/../bad.mbc"));

  const merged = mergeAppRegistry(builtin, external);
  assert.deepEqual(
    merged.map((app) => app.id),
    ["counter", "settings", "extra", "second"],
  );
});
