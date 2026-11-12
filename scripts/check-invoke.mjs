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

function walkRs(dir, out = []) {
  for (const name of readdirSync(dir)) {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) walkRs(path, out);
    else if (name.endsWith(".rs")) out.push(path);
  }
  return out;
}

const called = new Map();
const mentioned = new Set();
const scripts = [...walk(join(root, "src")), ...walk(join(root, "src-tauri/src/injected"))];
for (const file of scripts) {
  const source = readFileSync(file, "utf8");
  for (const match of source.matchAll(/invoke\(\s*"([a-z0-9_]+)"/g)) {
    const name = match[1];
    if (!called.has(name)) called.set(name, new Set());
    called.get(name).add(file.replace(/\\/g, "/").slice(root.length + 1));
  }
  // The injected script reaches some commands through small wrappers, so a name that
  // appears as a literal anywhere counts as reached.
  for (const match of source.matchAll(/["'`]([a-z0-9_]+)["'`]/g)) mentioned.add(match[1]);
}
// Part of the key and find bridge lives in rust string literals, and a few commands are
// called straight from rust instead of from a page.
const rustSources = walkRs(join(root, "src-tauri/src"))
  .filter((file) => !file.endsWith("main.rs"))
  .map((file) => readFileSync(file, "utf8"))
  .join("\n");
for (const match of rustSources.matchAll(/invoke\(\s*['"]([a-z0-9_]+)['"]/g)) mentioned.add(match[1]);

function calledFromRust(name) {
  const withoutDefinition = rustSources.replace(
    new RegExp("(async\\s+)?fn\\s+" + name + "\\s*\\(", "g"),
    "",
  );
  return new RegExp("\\b" + name + "\\(").test(withoutDefinition);
}

const unknown = [...called.keys()].filter((name) => !registered.has(name));
const unused = [...registered].filter(
  (name) => !called.has(name) && !mentioned.has(name) && !calledFromRust(name),
);

for (const name of unknown) {
  console.error("FAIL " + name + " is called from " + [...called.get(name)].join(", ") + " but not registered in main.rs");
}
console.log("commands: " + registered.size + " registered, " + called.size + " called from the ui");
if (unused.length) console.log("     registered but never called from the ui: " + unused.join(", "));
if (unknown.length) process.exit(1);
console.log("every invoked command exists");
