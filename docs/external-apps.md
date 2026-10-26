# External Apps on the SD card

Micro OS can install Apps without rebuilding or re-flashing firmware: put
compiled MBC images in the card's top-level `/apps` directory and they appear
in the Home launcher after the built-in Apps.

The feature is available on all three hosts. Only the ESP32 has a real card;
macOS and the Web Player use a simulated card at the same logical path.

| Host | External App location | How the list is found |
| --- | --- | --- |
| ESP32 (`micro-os-esp32`) | `/sdcard/apps/*.mbc` (card root `/apps`) | FATFS directory scan at Home boot |
| macOS (`micro-host-sdl`) | `$MICROS_HOME/sdcard/apps/*.mbc` (default `~/.micros/sdcard/apps`) | directory scan at launch |
| Web Player | `/sdcard/apps/index.json` + `/sdcard/apps/*.mbc` | HTTP manifest at boot |

A missing card, missing `/apps` directory, or missing Web `index.json` is
ignored: the launcher simply shows the built-in Apps and boot continues.

## Install an App

```bash
npm run build:app
mkdir -p /Volumes/YOUR_CARD/apps
cp apps/sd-demo/dist/sd-demo.mbc /Volumes/YOUR_CARD/apps/
```

The sample `apps/sd-demo` is not a built-in App; it exists to exercise the
external path. Any MBC produced by `microc` works, for example:

```bash
cargo run -p micro-compiler --bin microc -- apps/counter/app.ts /tmp/counter.mbc
```

For macOS, copy the same file to `~/.micros/sdcard/apps/`. To start from a clean
simulated card:

```bash
mkdir -p ~/.micros/sdcard/apps
cp apps/sd-demo/dist/sd-demo.mbc ~/.micros/sdcard/apps/
```

For the Web Player, put the images under `products/micro-web-player/public/sdcard/apps/`
and describe them in `index.json`:

```json
{
  "apps": ["sd-demo.mbc"]
}
```

`npm run build:web` copies `public/` into the production bundle. The manifest is
fetched from `/sdcard/apps/index.json`; names are bare `.mbc` file names and are
validated before a request is made.

## Launcher order and the eight-slot limit

The host registry always lists built-in Apps first (in install-partition order),
then external Apps sorted by file name. The shell has eight fixed slots
(`apps/shell/app.ts`), so at most eight Apps are listed. If the built-ins fill
the list, external Apps are skipped and the device log says the launcher is
full.

## Same-name Apps: built-ins win

Collision detection uses the manifest **id**, compared case-insensitively — not
the display name. The policy is deliberately conservative:

- A built-in App always wins. An SD file whose `id` matches `counter`,
  `settings`, `momo`, … is skipped and logged; it never shadows the trusted
  image.
- `id: "shell"` is reserved and is always rejected, even if the partition no
  longer contains it.
- If two SD files claim the same id, the first file in sorted order wins; the
  rest are skipped and logged.
- Invalid file names, malformed MBCs, unsupported versions, unsafe manifest ids
  and files past the eight-slot cap are skipped without stopping the OS. An
  external id must be 1–31 ASCII characters, start with a letter or digit, and
  use only letters, digits, `_`, `-` or `.` — the same rule as the per-App
  storage namespace.

On ESP32 the external list is rebuilt before every Home boot, so if the card was
mounted at startup, removing or reinserting it while an App runs is reflected
when you return Home. A card inserted after a boot that had no card needs a
reboot: the FATFS mount is initialized once. macOS and the Web Player scan at
host start.

Replacing a built-in App with an SD copy is intentionally not supported. If that
becomes a requirement, it should be an explicit opt-in (a separate "override"
flag or a Settings action) rather than the default trust policy.

## Failure behavior

The launcher only reads each MBC's header and metadata section. Full validation
happens when the App is launched: a corrupt/unsupported image fails
`micro_runtime_create`, the host logs the error and returns to Home. The shell
is never replaced by an external image, so a bad SD App cannot brick the
launcher.

External Apps are **not signed**. The manifest still declares host capabilities
and the runtime enforces them, and per-App `storage` stays scoped to
`/sdcard/micros/apps/<id>/`, but an SD App can request any capability the
manifest names. Treat `/apps` as a developer/trusted-card feature until signing
and install-time grants exist.
