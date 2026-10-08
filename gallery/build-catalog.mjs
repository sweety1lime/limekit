// Regenerates gallery/catalog.json from the packs under gallery/packs/*/.
// Each pack folder holds one `.limekit` (its manifest supplies name/author/type/
// license/id), an optional `preview.jpg`, and an optional `tags.json` (a string
// array). SHA-256 and size are computed here, so the catalog is never hand-
// edited. Run after adding a pack: `node gallery/build-catalog.mjs`.
//
// CI regenerates and fails if the committed catalog is stale (see the workflow),
// which keeps every entry's checksum honest.

import { readdirSync, statSync, readFileSync, writeFileSync, existsSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { createHash } from "node:crypto";

const PACKS_DIR = fileURLToPath(new URL("./packs/", import.meta.url));
const CATALOG = fileURLToPath(new URL("./catalog.json", import.meta.url));
const RAW = "https://raw.githubusercontent.com/sweety1lime/limekit/master/gallery";

function sha256(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

/** Reads manifest.json out of a `.limekit` (a zip) using the system `unzip`. */
function readManifest(packPath) {
  const out = execFileSync("unzip", ["-p", packPath, "manifest.json"]);
  return JSON.parse(out.toString("utf8"));
}

const packs = [];
for (const id of readdirSync(PACKS_DIR)) {
  const dir = `${PACKS_DIR}/${id}`;
  if (!statSync(dir).isDirectory()) continue;

  const files = readdirSync(dir);
  const pack = files.find((f) => f.endsWith(".limekit"));
  if (!pack) {
    console.error(`skip ${id}: no .limekit file`);
    continue;
  }
  const packPath = `${dir}/${pack}`;
  const manifest = readManifest(packPath);

  const entry = {
    id: manifest.id,
    name: manifest.name,
    author: manifest.author,
    type: manifest.type,
    license: manifest.license,
    sha256: sha256(packPath),
    size: statSync(packPath).size,
  };
  if (files.includes("preview.jpg")) entry.preview = `${RAW}/packs/${id}/preview.jpg`;
  entry.download_url = `${RAW}/packs/${id}/${pack}`;
  entry.tags = existsSync(`${dir}/tags.json`)
    ? JSON.parse(readFileSync(`${dir}/tags.json`, "utf8"))
    : [];

  packs.push(entry);
}

packs.sort((a, b) => a.id.localeCompare(b.id));
writeFileSync(CATALOG, JSON.stringify({ version: 1, packs }, null, 2) + "\n");
console.log(`✓ catalog.json: ${packs.length} пак(ов).`);
