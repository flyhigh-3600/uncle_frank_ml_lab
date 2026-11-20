import { cp, mkdir, rm, readdir } from "node:fs/promises";
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
const copyTree = async (from, to) => {
  await mkdir(to, { recursive: true });
  for (const entry of await readdir(from, { withFileTypes: true })) {
    const source = path.join(from, entry.name);
    const target = path.join(to, entry.name);
    if (entry.isDirectory()) await copyTree(source, target);
    else await cp(source, target);
  }
};
await copyTree(path.join(root, "../examples"), path.join(dist, "examples"));
console.log(`Built static editor at ${dist}`);
