# Mix — quick start

1. Unzip anywhere you can write to (not Program Files; the app stores data next to itself).
2. Run `mix.exe`. On Windows, SmartScreen may warn "unknown publisher" — click **More info → Run anyway**.
3. Settings, cache, logs and your tabs live in `UserData\` next to the exe. Back up or move the whole folder and everything travels with it.
4. Closing the window hides Mix to the tray. Quit from the tray menu, otherwise it keeps running.
5. Hotkeys: `Alt+Shift+←/→/↑/↓` dock the window; `Ctrl+T/W/L/R`, `Ctrl+Tab`, `Ctrl+±`, `F5` work as in any browser.
6. Preferences: the `=` button in the address bar (theme, startup, and the list of loaded extensions).

## Extensions (Windows)

Drop **either** into `crx\` and restart:

- an unpacked extension folder (anything with `manifest.json` at its root, e.g. a folder from Chrome's "Load unpacked"), or
- a `.crx` file — it gets unpacked into `UserData\extensions\`.

Only Manifest V3 works; MV2 extensions show as `skipped` in Preferences → Extensions.
Some extensions only act on their own site (BewlyCat touches bilibili, SponsorBlock only video pages).

## Userscripts

Drop `*.user.js` files into `javascript\` and restart. They are injected by their `@match` rules.

## If something breaks

The log lives in `UserData\logs\mix.log`; send me that file plus what you did.
