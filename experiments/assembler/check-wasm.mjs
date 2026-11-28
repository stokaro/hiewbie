import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const path = process.argv[2];
assert(path, "Pass the linked hiewbie_assembler_probe.wasm file");
const { instance } = await WebAssembly.instantiate(await readFile(path), {});
assert.equal(instance.exports.verify_assembler_probe(), 1);
console.log("WASM assembled the fixed text corpus without host imports");
