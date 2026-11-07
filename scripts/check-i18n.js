// Reports screen strings that have no entry in the zh dictionary. The i18n table
// is keyed by the English text, so a missing entry fails silently at runtime -
// this script is the check that makes it visible.
//
//   node scripts/check-i18n.js
//
// Exits 1 when something is missing, so it can gate a release.

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
    else if (entry.name.endsWith(".js")) files.push(full);
  }
};
walk(src);

const used = new Map();
for (const file of files) {
  if (file.endsWith(path.sep + "i18n.js")) continue;
  const text = fs.readFileSync(file, "utf8");
  const re = /\bt\(\s*"((?:[^"\\]|\\.)*)"\s*\)|\bt\(\s*'((?:[^'\\]|\\.)*)'\s*\)/g;
  let match;
  while ((match = re.exec(text))) {
    const key = match[1] !== undefined ? match[1] : match[2];
    if (!key) continue;
    if (!used.has(key)) used.set(key, new Set());
    used.get(key).add(path.relative(root, file));
  }
}

const missing = [...used.keys()].filter((key) => !dict.has(key)).sort();
const unused = [...dict].filter((key) => !used.has(key)).sort();

console.log("keys in dictionary: " + dict.size);
console.log("keys used by the ui:  " + used.size);
console.log("");
if (missing.length) {
  console.log("missing " + missing.length + ":");
  for (const key of missing) console.log('  "' + key + '"  <- ' + [...used.get(key)].join(", "));
  console.log("");
} else {
  console.log("no missing keys");
}
if (unused.length) {
  console.log("unused dictionary entries: " + unused.length);
  for (const key of unused.slice(0, 20)) console.log('  "' + key + '"');
  if (unused.length > 20) console.log("  ... " + (unused.length - 20) + " more");
}
process.exit(missing.length ? 1 : 0);
