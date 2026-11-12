// Cross-checks the ids a page script asks for against the ids its html defines.
// A typo in getElementById is silent: the element is null and the handler is never
// wired, which looks exactly like a dead button.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

const PAGES = [
  ["src/index.html", "src/app.js"],
  ["src/overlay.html", "src/overlay.js"],
  ["src/pages/history.html", "src/pages/history.js"],
  ["src/pages/bookmarks.html", "src/pages/bookmarks.js"],
  ["src/pages/downloads.html", "src/pages/downloads.js"],
  ["src/pages/extensions.html", "src/pages/extensions.js"],
  ["src/pages/scripts.html", "src/pages/scripts.js"],
  ["src/pages/newtab.html", "src/pages/newtab.js"],
  ["src/pages/settings.html", "src/pages/settings.js"],
  ["src/pages/sidebar.html", "src/pages/sidebar.js"],
];

const ids = (html) => new Set([...html.matchAll(/\bid="([^"]+)"/g)].map((m) => m[1]));

function wanted(js) {
  const found = new Set();
  for (const m of js.matchAll(/getElementById\(\s*"([^"]+)"/g)) found.add(m[1]);
  for (const m of js.matchAll(/querySelector\(\s*"#([^"\s.\[:]+)/g)) found.add(m[1]);
  return found;
}

function created(js) {
  const found = new Set();
  for (const m of js.matchAll(/\.id\s*=\s*"([^"]+)"/g)) found.add(m[1]);
  return found;
}

let failures = 0;
for (const [htmlPath, jsPath] of PAGES) {
  const have = ids(readFileSync(join(root, htmlPath), "utf8"));
  const js = readFileSync(join(root, jsPath), "utf8");
  const ask = wanted(js);
  const made = created(js);
  const missing = [...ask].filter((id) => !have.has(id) && !made.has(id));
  if (missing.length) {
    failures += 1;
    console.error("FAIL " + jsPath + " asks for ids its page does not define: " + missing.join(", "));
  } else {
    console.log("ok   " + jsPath + " (" + ask.size + " ids)");
  }
  const unused = [...have].filter((id) => !ask.has(id) && !js.includes('"' + id + '"'));
  if (unused.length) console.log("     " + htmlPath + " defines ids nothing refers to: " + unused.join(", "));
}

if (failures) process.exit(1);
console.log("all page scripts and their html agree on ids");
