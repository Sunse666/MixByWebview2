# M0 report

Fill one section per experiment. A section is done only when "Result" is written
and "Fallback" is either not needed or described.

Environment used for every row:

| Field | Value |
| --- | --- |
| Windows | |
| macOS | |
| Linux (X11) | |
| Linux (Wayland) | |
| WebView2 / WebKitGTK version | |
| Probe commit | |

## E1 — MV3 extension + DNR

- Steps: unpack an MV3 extension into `extensions/`, run `cargo run -- --exp e1`, open devtools.
- Pass: content script runs and `chrome.runtime.id` is defined; uBlock Origin Lite blocks requests on a test page (rule counter > 0).
- Result:
- Fallback:

## E2 — two child webviews

- Steps: `--exp e2`, type and scroll in both panes, resize the window.
- Pass: no interference, focus follows clicks, both follow resize.
- Result:
- Fallback:

## E3 — init script in all frames

- Steps: `--exp e3`, read the console lines from the top frame and both iframe levels.
- Pass: `__PROBE` exists in every frame and `ready` is `loading` (script beat page scripts).
- Result:
- Fallback:

## E4 — overlay layer

- Steps: `--exp e4`, check paint order, type in the overlay input, click the tab behind it.
- Pass: overlay paints above the tab, keeps focus, no flicker when shown.
- Result:
- Fallback:

## E5 — download and upload

- Steps: `--exp e5`, click the download link, then pick a file.
- Pass: download event fires and the file lands on disk; file picker opens.
- Result:
- Fallback:

## E6 — IME

- Steps: `--exp e6`, type Chinese into both panes with the input method on, switch focus between them.
- Pass: no dropped characters, candidate window follows the caret.
- Result:
- Fallback:

## E7 — mixed DPI

- Steps: `--exp e7`, drag the window between a 100% and a 150% monitor.
- Pass: child webviews keep aligned with the chrome strip, no 1px gap, no offset.
- Result:
- Fallback:

## E9 — platform capabilities

- Steps: `--exp e9` on each platform and session type.
- Pass: window self-positioning works or is documented as unsupported; monitors reported.
- Result:
- Fallback:

## Decision

- Architecture frozen on:
- Platforms in scope:
- Known gaps accepted:
