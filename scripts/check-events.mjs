// Every event a page listens for has to have something that emits it.
//
// A listener whose event nobody sends fails silently: the view keeps showing whatever it
// was handed when it opened, and nothing anywhere reports an error. Two of those were live
// in the tree - the sidebar's history panel never heard about a deletion, and the toolbar
// listened for an extension change that was never sent - so this is the check that makes
// the next one visible.
import { readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

function walk(dir, match, out = []) {
  for (const name of readdirSync(dir)) {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) walk(path, match, out);
    else if (match(name)) out.push(path);
  }
  return out;
}

const listened = new Map();
const pages = [...walk(join(root, "src"), (name) => name.endsWith(".js"))];
for (const file of pages) {
  const source = readFileSync(file, "utf8");
  // A name built at run time cannot be checked; only literals are collected.
  for (const match of source.matchAll(/\blisten\(\s*["']([\w:.-]+)["']/g)) {
    const name = match[1];
    if (!listened.has(name)) listened.set(name, new Set());
    listened.get(name).add(file.replace(/\\/g, "/").slice(root.length + 1));
  }
}

// Rust is where the events are sent from: emit(), emit_to(), or a name handed to emit
// through a variable whose value is a literal somewhere in the same tree.
const rust = walk(join(root, "src-tauri/src"), (name) => name.endsWith(".rs"))
  .map((file) => readFileSync(file, "utf8"))
  .join("\n");

const dead = [...listened.keys()].filter((name) => !rust.includes('"' + name + '"')).sort();

for (const name of dead) {
  console.error(`FAIL "${name}" is listened for by ${[...listened.get(name)].join(", ")} and emitted nowhere`);
}
console.log(`events: ${listened.size} listened for, ${dead.length} with no sender`);
if (dead.length) process.exit(1);
console.log("every listened event has a sender");
