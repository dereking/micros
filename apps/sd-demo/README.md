# SD Demo

A minimal external App used by the SD-card launcher tests. It is not part of the
built-in registry.

```bash
npm run build:app
cp apps/sd-demo/dist/sd-demo.mbc /Volumes/YOUR_CARD/apps/
```

On macOS, copy it to `~/.micros/sdcard/apps/`; in the Web Player, serve it from
`/sdcard/apps/sd-demo.mbc` and list it in `/sdcard/apps/index.json`. See
[docs/external-apps.md](../../docs/external-apps.md).
