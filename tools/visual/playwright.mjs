/**
 * Resolves the `playwright` module for the tools in this directory. When
 * Playwright is installed globally via mise rather than in a local
 * package.json, bare `import("playwright")` fails in ESM, so global npm and
 * mise install paths are tried too.
 */

import { execFileSync } from "node:child_process";
import fs from "node:fs";
import { createRequire } from "node:module";
import os from "node:os";
import path from "node:path";

const require = createRequire(import.meta.url);

export async function loadPlaywright() {
  try {
    return await import("playwright");
  } catch {
    // Fall back to global npm / mise installation paths.
  }

  const candidates = [];

  if (process.env.PLAYWRIGHT_MODULE_PATH) {
    candidates.push(process.env.PLAYWRIGHT_MODULE_PATH);
  }

  try {
    const npmRoot = execFileSync("npm", ["root", "-g"], {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "ignore"],
    }).trim();
    if (npmRoot) {
      candidates.push(path.join(npmRoot, "playwright"));
    }
  } catch {
    // npm may not be on PATH in minimal environments; continue checking mise paths.
  }

  candidates.push(
    path.join(
      os.homedir(),
      ".local/share/mise/installs/npm-playwright/latest/node_modules/playwright"
    )
  );

  for (const dir of (process.env.PATH || "").split(path.delimiter)) {
    if (!dir) continue;
    const binCandidate = path.join(dir, "playwright");
    if (fs.existsSync(binCandidate)) {
      try {
        const realBin = fs.realpathSync(binCandidate);
        candidates.push(path.resolve(path.dirname(realBin), "..", "playwright"));
      } catch {
        // Ignore unreadable or broken symlink entries.
      }
    }
  }

  const tried = [];
  for (const candidate of candidates) {
    if (tried.includes(candidate)) continue;
    tried.push(candidate);
    if (fs.existsSync(candidate)) {
      return require(candidate);
    }
  }

  throw new Error(
    `Cannot resolve 'playwright' module. Checked standard import and: ${tried.join(", ")}`
  );
}
