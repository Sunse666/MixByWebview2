// Reports screen strings that have no entry in the zh dictionary, and dictionary entries
// that nothing can ever show.
//
//   node scripts/check-i18n.js
//
// The i18n table is keyed by the English text, so a missing entry fails silently at
// runtime - this script is the check that makes it visible.
//
// "Used" is deliberately generous: a key counts as used when it appears inside t(...) or
// when the same string appears anywhere in the sources as a literal. Text is often
// translated through a variable (t(label) over a list of labels, menu definitions passed
// around as data), and a narrower rule would report those as dead and invite their
// deletion. Exits 1 when something is missing, so it can gate a release.

const fs = require("fs");
const path = require("path");

const root = path.join(__dirname, "..");
const src = path.join(root, "src");

const dict = new Set();
const i18n = fs.readFileSync(path.join(src, "i18n.js"), "utf8");
for (const line of i18n.split(/\r?\n/)) {
  // Keys are either quoted ("Close tab") or bare identifiers (Settings).
  const match = /^\s*(?:"((?:[^"\\]|\\.)*)"|([A-Za-z_$][\w$]*))\s*:/.exec(line);
  if (match) dict.add(match[1] !== undefined ? match[1] : match[2]);
}

const files = [];
const walk = (dir) => {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) walk(full);
    else if (/\.(js|html)$/.test(entry.name)) files.push(full);
  }
};
walk(src);
// The Rust side holds strings the pages translate through a variable: translation status,
// script injection timing, download state. Without them the report calls live entries dead.
const rustFiles = [];
(function walkRust(dir) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) walkRust(full);
    else if (entry.name.endsWith(".rs")) rustFiles.push(full);
  }
})(path.join(root, "src-tauri", "src"));

const calls = new Map();
const literals = new Set();
const unescape = (value) => value.replace(/\\(.)/g, "$1");

for (const file of files) {
  if (file.endsWith(path.sep + "i18n.js")) continue;
  const text = fs.readFileSync(file, "utf8");

  const re = /\bt\(\s*(?:"((?:[^"\\]|\\.)*)"|'((?:[^'\\]|\\.)*)')\s*\)/g;
  let match;
  while ((match = re.exec(text))) {
    const key = unescape(match[1] !== undefined ? match[1] : match[2]);
    if (!key) continue;
    if (!calls.has(key)) calls.set(key, new Set());
    calls.get(key).add(path.relative(root, file));
  }

  // Every literal in the file, so a key passed around as data still counts. Escapes are
  // decoded too: page titles and labels are often written with \u sequences.
  const strings = text.match(/"(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*'/g) || [];
  for (const raw of strings) {
    literals.add(unescape(raw.slice(1, -1)));
  }
}

for (const file of rustFiles) {
  const text = fs.readFileSync(file, "utf8");
  for (const raw of text.match(/"((?:[^"\\]|\\.)*)"/g) || []) {
    literals.add(unescape(raw.slice(1, -1)));
  }
}

const used = new Set([...calls.keys()].filter((key) => dict.has(key)));
for (const key of dict) {
  if (literals.has(key)) used.add(key);
}

const missing = [...calls.keys()].filter((key) => !dict.has(key)).sort();
const unused = [...dict].filter((key) => !used.has(key)).sort();

console.log("keys in dictionary: " + dict.size);
console.log("keys used by the ui:  " + used.size);
console.log("");
if (missing.length) {
  console.log("missing " + missing.length + ":");
  for (const key of missing) console.log('  "' + key + '"  <- ' + [...calls.get(key)].join(", "));
  console.log("");
} else {
  console.log("no missing keys");
}
if (unused.length) {
  // A hint, not a to-do list, and two attempts at pruning on it failed for different
  // reasons: text can be translated through a variable whose value comes from Rust or from
  // a data file (translation status, a userscript's @run-at, download state), and it can be
  // a text node in html ("Allow" lives in overlay.html, not in a quoted string). A static
  // scan cannot tell those from dead entries, and the dictionary is small enough that the
  // wrong deletions cost more than the dead weight.
  console.log("dictionary entries no page mentions: " + unused.length + " (check before removing)");
  for (const key of unused) console.log('  "' + key + '"');
}
process.exit(missing.length ? 1 : 0);