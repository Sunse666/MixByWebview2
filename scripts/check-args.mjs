// Compares the arguments a page passes to invoke() with the parameters the command takes.
//
// A wrong name here fails silently: the command runs, receives nothing for that parameter,
// and the page swallows the error. Nothing else in the tree can see it - the command exists,
// the call is spelled right, and the feature just does not work.
//
// Only literal keys are checked. A page that builds its arguments (spread, computed key, a
// variable holding an object) is reported as skipped rather than guessed at.
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

/** Parameters of every #[tauri::command], by command name. */
function commandParameters() {
  const commands = new Map();
  const files = walk(join(root, "src-tauri/src"), (name) => name.endsWith(".rs"));
  for (const file of files) {
    const source = readFileSync(file, "utf8");
    for (const match of source.matchAll(/#\[tauri::command\][\s\S]{0,40}?pub async fn (\w+)\s*\(([\s\S]*?)\)\s*->/g)) {
      const [, name, raw] = match;
      const params = [];
      for (const part of raw.split(",")) {
        const piece = part.trim();
        if (!piece || piece.startsWith("//")) continue;
        const declared = /^(\w+)\s*:\s*(.*)$/.exec(piece);
        if (!declared) continue;
        const [, param, type] = declared;
        // Tauri fills these in; a page never sends them. Matched on the type, because a
        // command may well take a `window: String` of its own next to the injected window.
        if (/^(AppHandle|State<|tauri::Window|tauri::Webview)/.test(type)) continue;
        // `param: Option<T>` may be left out by a caller.
        params.push({ name: param, optional: type.startsWith("Option<") });
      }
      commands.set(name, params);
    }
  }
  return commands;
}

/** Argument keys at every invoke() call site, where they are written out literally. */
function callSites() {
  const sites = [];
  const files = [
    ...walk(join(root, "src"), (name) => name.endsWith(".js")),
    ...walk(join(root, "src-tauri/src/injected"), (name) => name.endsWith(".js")),
  ];
  for (const file of files) {
    const source = readFileSync(file, "utf8");
    const label = file.replace(/\\/g, "/").slice(root.length + 1);
    for (const match of source.matchAll(/invoke\(\s*"([a-z0-9_]+)"\s*(,\s*\{)?/g)) {
      const [, name, opens] = match;
      if (!opens) continue;
      // Walk the object literal to its closing brace, so nested objects stay together.
      let depth = 0;
      let index = match.index + match[0].length - 1;
      let text = "";
      for (; index < source.length; index += 1) {
        const ch = source[index];
        if (ch === "{") depth += 1;
        else if (ch === "}") {
          depth -= 1;
          if (depth === 0) break;
        }
        text += ch;
      }
      const line = source.slice(0, match.index).split("\n").length;
      sites.push({ name, text, label, line });
    }
  }
  return sites;
}

/** The literal top-level keys of an object literal body. */
function topLevelKeys(body) {
  const keys = [];
  let depth = 0;
  let chunk = "";
  const flush = () => {
    const piece = chunk.trim();
    chunk = "";
    if (!piece) return;
    if (piece.startsWith("...") || piece.startsWith("[")) return;
    const found = /^([A-Za-z_$][\w$]*)\s*(:|\()/.exec(piece);
    if (found) {
      keys.push(found[1]);
      return;
    }
    // `{ id, to }` - shorthand for `{ id: id, to: to }`, and how most of the strip talks
    // to the backend.
    if (/^[A-Za-z_$][\w$]*$/.test(piece)) keys.push(piece);
  };
  for (const ch of body.slice(1)) {
    if ("{[(".includes(ch)) depth += 1;
    if ("}])".includes(ch)) depth -= 1;
    if (ch === "," && depth === 0) {
      flush();
      continue;
    }
    chunk += ch;
  }
  flush();
  return keys;
}

const camel = (name) => name.replace(/_([a-z])/g, (_, c) => c.toUpperCase());

const commands = commandParameters();
const problems = [];
let checked = 0;
const skipped = new Set();
const passed = new Map();
const seen = new Set();

for (const site of callSites()) {
  const params = commands.get(site.name);
  if (!params) continue; // check-invoke already reports unknown commands.
  seen.add(site.name);
  const keys = topLevelKeys(site.text);
  if (!keys.length) {
    skipped.add(site.name);
    continue;
  }
  checked += 1;
  if (!passed.has(site.name)) passed.set(site.name, new Set());
  for (const key of keys) passed.get(site.name).add(key);
  const known = new Set(params.flatMap((param) => [param.name, camel(param.name)]));
  for (const key of keys) {
    if (known.has(key)) continue;
    problems.push(
      `${site.label}:${site.line}  ${site.name}  does not take "${key}"` +
        (params.length ? `  (takes ${params.map((param) => param.name).join(", ")})` : "  (takes nothing)"),
    );
  }
}

/** True when rust calls the command itself, so a page need not pass everything. */
function calledFromRust(name) {
  const files = walk(join(root, "src-tauri/src"), (entry) => entry.endsWith(".rs"));
  for (const file of files) {
    const source = readFileSync(file, "utf8").replace(
      new RegExp("(async\\s+)?fn\\s+" + name + "\\s*\\(", "g"),
      "",
    );
    if (new RegExp("\\b" + name + "\\(").test(source)) return true;
  }
  return false;
}

// The other half of the same failure: a parameter the command requires and no page ever
// passes. The call still resolves - it just never carries the value.
for (const name of seen) {
  if (skipped.has(name)) continue;
  if (calledFromRust(name)) continue;
  const given = passed.get(name) || new Set();
  for (const param of commands.get(name)) {
    if (param.optional) continue;
    if (given.has(param.name) || given.has(camel(param.name))) continue;
    problems.push(`src  ${name}  never passes "${param.name}"`);
  }
}

for (const problem of problems) console.error("FAIL " + problem);
console.log(`arguments: ${checked} call site(s) checked, ${skipped.size} built at run time`);
if (problems.length) {
  console.error(problems.length + " argument problem(s)");
  process.exit(1);
}
console.log("every argument matches a parameter, and every parameter is passed");
