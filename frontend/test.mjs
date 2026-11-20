import { assertSamples } from "./samples.js";
import { access, readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

assertSamples();
const root = path.dirname(fileURLToPath(import.meta.url));
for (const example of [
  "text_io.ufp", "portal_demo.ufp", "model_layers.ufp",
  "layers/layer_000.ufp", "notes.txt",
]) {
  const actual = await readFile(path.join(root, "../examples", example), "utf8");
  if (!actual.trim()) throw new Error(`Repository example is empty: ${example}`);
}
try {
  await access(path.join(root, "dist", "examples", "text_io.ufp"));
} catch {
  // Test suite also runs before build; bundle-copy is exercised by npm run build.
}
console.log("Frontend example and grammar-shape checks passed.");
