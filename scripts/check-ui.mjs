// Checks the UI's JavaScript (ui/js), as CI does: every file parses as a module, and
// every import names a file that exists and names that file exports.
// Run it from the repository's folder: node scripts/check-ui.mjs
import { execFileSync } from "node:child_process";
import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { dirname, join, relative } from "node:path";

const root = "ui/js";
const files = [];
(function walk(dir) {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) walk(p);
    else if (name.endsWith(".js")) files.push(p);
  }
})(root);

let failed = 0;
const fail = (msg) => { console.error(msg); failed++; };
const exportsOf = (text) => new Set([...text.matchAll(/^export (?:async function|function|const|let) ([\w$]+)/gm)].map((m) => m[1]));

for (const file of files) {
  const text = readFileSync(file, "utf8");
  try {
    // --input-type=module reads stdin as a module on every Node version.
    execFileSync(process.execPath, ["--input-type=module", "--check"], { input: text, stdio: ["pipe", "ignore", "pipe"] });
  } catch (e) {
    fail(`${file} doesn't parse:\n${e.stderr}`);
  }
  for (const m of text.matchAll(/^import \{([^}]*)\} from "([^"]+)";/gm)) {
    const target = join(dirname(file), m[2]);
    if (!existsSync(target)) { fail(`${file} imports ${m[2]}, which doesn't exist`); continue; }
    const have = exportsOf(readFileSync(target, "utf8"));
    for (const name of m[1].split(",").map((s) => s.trim()).filter(Boolean)) {
      if (!have.has(name)) fail(`${file} imports ${name} from ${relative(".", target)}, which doesn't export it`);
    }
  }
}
if (failed) process.exit(1);
console.log(`${files.length} files parse and their imports exist.`);
