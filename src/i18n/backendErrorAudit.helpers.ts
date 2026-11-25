import { readdirSync, readFileSync, statSync } from "node:fs";

/** Where a backend refusal key was found. */
export interface BackendKeyUse {
  key: string;
  file: string;
}

/**
 * Parts of the Rust tree whose messages a player never reads (see the root CLAUDE.md, rule 2): the
 * MCP server and the CLI speak to an agent or a modder, and test code speaks to nobody.
 */
const NOT_PLAYER_FACING = ["/mcp_server/", "/ofm-cli/", "/tests/", "/target/", "/benches/"];
const TEST_FILE = /(_tests?|tests?|testkit)\.rs$/;

/**
 * The `be.error.*` keys a Rust source file can send to the screen. The file's test module is cut
 * off (everything from the first `#[cfg(test)]`), and a literal that ends in a dot is a prefix the
 * code appends to, not a key.
 */
export function backendErrorKeysIn(source: string): string[] {
  const cut = source.indexOf("#[cfg(test)]");
  const production = cut === -1 ? source : source.slice(0, cut);
  const keys = new Set<string>();
  for (const match of production.matchAll(/"(be\.error\.[A-Za-z0-9_.]*[A-Za-z0-9_])"/g)) {
    keys.add(match[1]);
  }
  return [...keys];
}

function walk(directory: string, found: string[]): void {
  for (const entry of readdirSync(directory)) {
    const path = `${directory}/${entry}`;
    if (NOT_PLAYER_FACING.some((part) => `${path}/`.includes(part))) continue;
    if (statSync(path).isDirectory()) walk(path, found);
    else if (path.endsWith(".rs") && !TEST_FILE.test(path)) found.push(path);
  }
}

/** Every backend error key in the player-facing Rust source, with the file that sends it. */
export function collectBackendErrorKeys(rustRoots: string[]): BackendKeyUse[] {
  const files: string[] = [];
  for (const root of rustRoots) walk(root, files);
  return files.flatMap((file) =>
    backendErrorKeysIn(readFileSync(file, "utf-8")).map((key) => ({ key, file })),
  );
}
