// Finds interface text that never went through t(): Chinese written straight into a
// page, or into the chrome or an overlay. Anything found here shows up in one language
// only, which is exactly the bug the dictionary exists to prevent.
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../src", import.meta.url));
const CJK = /[\u4e00-\u9fff]/;

function walk(dir) {
  const out = [];
  for (const name of readdirSync(dir)) {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) out.push(...walk(path));
    else if (/\.(js|html)$/.test(name)) out.push(path);
  }
  return out;
}

const problems = [];
for (const file of walk(root)) {
  if (file.endsWith("i18n.js")) continue;
  const lines = readFileSync(file, "utf8").split("\n");
  lines.forEach((line, index) => {
    if (!CJK.test(line)) return;
    // Comments explain things to the next reader; they are not interface text.
    const code = line.replace(/\/\/.*$/, "").replace(/\/\*.*?\*\//g, "");
    if (!CJK.test(code)) return;
    // Text inside t("...") is translated on purpose.
    if (/\bt\(\s*["'`][^"'`]*[\u4e00-\u9fff]/.test(line)) return;
    problems.push(file.replace(/\\/g, "/").replace(/.*\/src\//, "src/") + ":" + (index + 1) + "  " + line.trim());
  });
}

if (problems.length) {
  for (const problem of problems) console.error("FAIL " + problem);
  console.error(problems.length + " line(s) of interface text bypass t()");
  process.exit(1);
}
console.log("ok   no hardcoded interface text");
