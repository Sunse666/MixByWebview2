// Guards the chrome page against the mistake that took the strip down once: the boot
// function was left defined but never called, so no tab, no toolbar and no address bar
// ever appeared, and nothing anywhere reported an error.
import { readFileSync } from "node:fs";

const source = readFileSync(new URL("../src/app.js", import.meta.url), "utf8");
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

if (problems.length) {
  for (const problem of problems) console.error("FAIL " + problem);
  process.exit(1);
}
console.log("ok   chrome page boots");
