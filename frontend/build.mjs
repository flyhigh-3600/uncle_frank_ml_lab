import { cp, mkdir, rm } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import path from "node:path";
import { assertSamples } from "./samples.js";

const root = path.dirname(fileURLToPath(import.meta.url));
const dist = path.join(root, "dist");
assertSamples();
await rm(dist, { recursive: true, force: true });
await mkdir(dist, { recursive: true });
for (const file of ["index.html", "app.css", "app.js", "samples.js"]) {
  await cp(path.join(root, file), path.join(dist, file));
}
console.log(`Built static editor at ${dist}`);
