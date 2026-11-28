import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { readFile, readdir, stat } from "node:fs/promises";
import { join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));
const hash = (bytes) => createHash("sha256").update(bytes).digest();
const sorted = (paths) => paths.sort((left, right) => Buffer.compare(Buffer.from(left), Buffer.from(right)));
const git = (...args) => execFileSync("git", args, { cwd: root, timeout: 30_000 });

// Same path + NUL + binary SHA-256 framing as fuzz/run.py. Source paths include
// tracked and nonignored new files, so a dirty working tree has an exact digest.
async function sourceDigest() {
  const names = git("ls-files", "--cached", "--others", "--exclude-standard", "-z", "--",
    "fuzz", "crates/hiewbie-core", "tests/fixtures", "Cargo.lock", "Cargo.toml")
    .toString("utf8").split("\0").filter(Boolean);
  const digest = createHash("sha256");
  for (const name of sorted([...new Set(names)])) {
    const path = join(root, name);
    const info = await stat(path).catch((error) => {
      if (error.code === "ENOENT") return null;
      throw error;
    });
    if (info?.isFile()) digest.update(Buffer.from(name + "\0")).update(hash(await readFile(path)));
  }
  return digest.digest("hex");
}

async function digestFiles(directory) {
  const files = [];
  async function visit(path) {
    for (const entry of await readdir(path, { withFileTypes: true })) {
      const child = join(path, entry.name);
      if (entry.isDirectory()) await visit(child);
      else if (entry.isFile()) files.push(child);
      else throw new Error(`Corpus contains a nonregular entry: ${child}`);
    }
  }
  await visit(directory);
  const digest = createHash("sha256");
  const names = sorted(files.map((path) => relative(directory, path).replaceAll("\\", "/")));
  for (const name of names) {
    digest.update(Buffer.from(name + "\0")).update(hash(await readFile(join(directory, name))));
  }
  return { sha256: digest.digest("hex"), files: names.length };
}

const report = {
  schema_version: 1,
  evidence: "wasm32-unknown-unknown corpus executed in Node; no browser UI claim",
  status: "running",
  node: process.version,
  results: [],
};
try {
  assert.equal(process.argv.length, 4, "Usage: node fuzz/check-wasm.mjs MODULE.wasm GENERATED_CORPUS");
  report.commit = git("rev-parse", "HEAD").toString("utf8").trim();
  report.working_tree_dirty = git("status", "--porcelain", "--untracked-files=all").length > 0;
  report.source_sha256 = await sourceDigest();
  report.config = JSON.parse(await readFile(join(root, "fuzz/config.json"), "utf8"));
  const corpus = resolve(process.argv[3]);
  report.initial_corpus = await digestFiles(corpus);
  const bytes = await readFile(process.argv[2]);
  report.wasm_sha256 = hash(bytes).toString("hex");
  const module = await WebAssembly.compile(bytes);
  assert.deepEqual(WebAssembly.Module.imports(module), [], "Replay must not import host functions");
  const instance = await WebAssembly.instantiate(module, {});
  for (const [index, target] of report.config.targets.entries()) {
    const result = { target, status: "running", initial_corpus: await digestFiles(join(corpus, target)) };
    report.results.push(result);
    assert(result.initial_corpus.files > 0, `${target}: no source-generated seed files`);
    result.cases = instance.exports.hiewbie_replay_fuzz_target(index);
    assert(Number.isInteger(result.cases) && result.cases > 0, `${target}: no corpus cases replayed`);
    result.status = "passed";
  }
  assert.equal(instance.exports.hiewbie_replay_fuzz_target(report.config.targets.length), 0);
  report.status = "passed";
} catch (error) {
  report.status = "failed";
  report.error = error instanceof Error ? error.message : String(error);
  for (const result of report.results) if (result.status === "running") result.status = "failed";
  process.exitCode = 1;
}
console.log(JSON.stringify(report, null, 2));
