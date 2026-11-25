// Vitest stubs every CSS import to "", even with `?raw`, so a test that reads `src/App.css` has to
// use node:fs. The repo ships without @types/node on purpose; this declares the calls tests need.
declare module "node:fs" {
  export function readFileSync(path: string, encoding: "utf-8"): string;
  export function readdirSync(path: string): string[];
  export function statSync(path: string): { isDirectory(): boolean };
}
