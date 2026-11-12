// Compares the commands the frontend calls with the ones the backend registers.
// A call to a command that does not exist rejects at runtime, and every page ends
// its invokes with .catch(() => {}), so the only symptom is a control that does nothing.
import { readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

function walk(dir, out = []) {
  for (const name of readdirSync(dir)) {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) walk(path, out);
    else if (name.endsWith(".js")) out.push(path);
  }
  return out;
}

const registered = new Set(
  [...readFileSync(join(root, "src-tauri/src/main.rs"), "utf8").matchAll(/ipc::([a-z0-9_]+)\s*,/g)].map((m) => m[1]),
);

const called = new Map();
for (const file of [...walk(join(root, "src")), ...walk(join(root, "src-tauri/src/injected"))]) {
  const source = readFileSync(file, "utf8");
  for (const match of source.matchAll(/invoke\(\s*"([a-z0-9_]+)"/g)) {
    const name = match[1];
    if (!called.has(name)) called.set(name, new Set());
    called.get(name).add(file.replace(root + "/", "").replace(/\\/g, "/"));
  }
}

const unknown = [...called.keys()].filter((name) => !registered.has(name));
const unused = [...registered].filter((name) => !called.has(name));

for (const name of unknown) {
  console.error("FAIL " + name + " is called from " + [...called.get(name)].join(", ") + " but not registered in main.rs");
}
console.log("commands: " + registered.size + " registered, " + called.size + " called from the ui");
if (unused.length) console.log("     registered but never called from the ui: " + unused.join(", "));
if (unknown.length) process.exit(1);
console.log("every invoked command exists");
