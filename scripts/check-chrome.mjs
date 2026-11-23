// Guards the chrome page against the mistake that took the strip down once: the boot
// function was left defined but never called, so no tab, no toolbar and no address bar
// ever appeared, and nothing anywhere reported an error.
//
// It also guards the strip's height, which fails just as silently in the other direction:
// the height is measured from the page and the webview is sized to the result, so anything
// that ties the page's own height to that webview turns the measurement into a tautology.
import { readFileSync } from "node:fs";

const source = readFileSync(new URL("../src/app.js", import.meta.url), "utf8");
const css = readFileSync(new URL("../src/style.css", import.meta.url), "utf8");
const problems = [];

if (!/^boot\(\)/m.test(source)) {
  problems.push("app.js defines boot() but never calls it");
}
if (!/export /.test(source) === false) {
  problems.push("app.js grew an export: it is a page script, not a module export");
}

// Handlers the page promises to install before the first keystroke or click.
for (const anchor of ['listen("tabs:changed"', 'listen("settings:changed"', "renderTabs()"]) {
  if (!source.includes(anchor)) problems.push(`app.js no longer wires ${anchor}`);
}

// The strip sizes its own webview: fitStrip() measures the body, chrome_fit sends the number
// back as bounds. Each half of that loop is load-bearing, and both fail without a word.
if (!/function fitStrip\(\)/.test(source)) {
  problems.push("app.js lost fitStrip(): nothing reports the strip's height any more");
}
// The row decides its own visibility and then reports the height, in that order: `fitStrip`
// measures the body, so it has to run after the row is shown or hidden, not before.
//
// Matched as "an assignment to bar.hidden followed at some point by a fitStrip() call in
// the same function", rather than as the exact line the first version of this check saw:
// the condition grew a second setting (`bookmark_bar_mode` can hide the row while
// `show_bookmarks` is true) and a check that fails when the expression changes is testing
// the spelling, not the order it was written to protect.
const hides = source.indexOf("bar.hidden =");
const fits = source.indexOf("fitStrip();", hides);
if (hides < 0 || fits < 0) {
  problems.push("renderBookmarks does not refit the strip: the bookmarks row would be clipped");
}
// A body pinned to the viewport measures the height the webview was given, not the height the
// content needs, so the loop agrees with itself and never corrects anything.
if (/html\s*,\s*body\s*\{[^}]*\bheight:\s*100%/.test(css)) {
  problems.push("style.css pins html/body to the viewport: fitStrip would read back its own value");
}
// `[hidden]` is a user-agent rule; the author `display: flex` on the row beats it.
if (!/\.bookmarks\[hidden\][^{]*\{\s*display:\s*none/.test(css)) {
  problems.push("style.css lost .bookmarks[hidden]: the row cannot be switched off");
}

if (problems.length) {
  for (const problem of problems) console.error("FAIL " + problem);
  process.exit(1);
}
console.log("ok   chrome page boots");
