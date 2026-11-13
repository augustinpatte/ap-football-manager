// Shared plumbing for the dev-only browser checks: launch Chrome headless, find its DevTools
// socket, and call CDP methods. Kept apart from any one check so the layout sweep and the
// interaction check connect the same way.

import { spawn } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";

/** Why this Node can't run a check, or null when it can. Node 22 brings the built-in WebSocket. */
export function nodeProblem(webSocket, version) {
  return typeof webSocket === "function"
    ? null
    : `the browser checks need Node 22 or later for the built-in WebSocket; you have ${version}`;
}

const CHROME_CANDIDATES = [
  "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
  "/Applications/Chromium.app/Contents/MacOS/Chromium",
  "google-chrome",
  "chromium",
  "chromium-browser",
];

/**
 * The Chrome to launch: `CHROME_PATH` when the caller names one, otherwise the first installed
 * candidate. Command names are left for the shell's PATH to resolve; absolute paths are checked.
 */
export function chromeCommand(env = process.env, exists = existsSync) {
  if (env.CHROME_PATH) return env.CHROME_PATH;
  return (
    CHROME_CANDIDATES.find((command) => !command.includes("/") || exists(command)) ??
    "google-chrome"
  );
}

/** Chrome in its own process group, so stopping it also stops the helpers that write the profile. */
export function launchChrome(profile, env = process.env) {
  const chrome = spawn(
    chromeCommand(env),
    [
      "--headless=new",
      "--disable-gpu",
      "--no-sandbox",
      "--remote-debugging-port=0",
      `--user-data-dir=${profile}`,
      "about:blank",
    ],
    { stdio: "ignore", detached: true },
  );
  // A failed start (no Chrome on PATH) is reported by whoever awaits devtools(), not thrown here.
  chrome.once("error", (error) => {
    chrome.startError = error;
  });
  return chrome;
}

/**
 * The page's DevTools socket. Chrome was asked for any free port (0) and writes the one it took to
 * DevToolsActivePort in its profile, so two runs at once never collide on a fixed port.
 */
export async function devtools(profile, chrome) {
  const portFile = join(profile, "DevToolsActivePort");
  for (let attempt = 0; attempt < 75; attempt += 1) {
    if (chrome.startError) throw chrome.startError;
    if (chrome.exitCode !== null) throw new Error(`Chrome exited (${chrome.exitCode}) on start`);
    if (existsSync(portFile)) {
      const [port] = readFileSync(portFile, "utf-8").split("\n");
      try {
        const targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
        const page = targets.find((target) => target.type === "page");
        if (page) return page.webSocketDebuggerUrl;
      } catch (error) {
        // The port file can appear a moment before the endpoint answers: say so, then retry.
        console.error(`waiting for Chrome's DevTools: ${error.message}`);
      }
    }
    await new Promise((resolve) => setTimeout(resolve, 200));
  }
  throw new Error("Chrome's DevTools endpoint never came up");
}

/** Stops Chrome's whole process group; one that has already gone is not an error. */
export function stopChrome(chrome) {
  try {
    process.kill(-chrome.pid);
  } catch (error) {
    if (error.code !== "ESRCH") throw error;
  }
}

/** A CDP connection: `call(method, params)` resolves with Chrome's reply. */
export function connect(url) {
  const socket = new WebSocket(url);
  let nextId = 0;
  const pending = new Map();
  socket.addEventListener("message", (event) => {
    const reply = JSON.parse(event.data);
    pending.get(reply.id)?.(reply);
    pending.delete(reply.id);
  });
  const call = (method, params = {}) =>
    new Promise((resolve) => {
      nextId += 1;
      pending.set(nextId, resolve);
      socket.send(JSON.stringify({ id: nextId, method, params }));
    });
  return new Promise((resolve) => socket.addEventListener("open", () => resolve({ socket, call })));
}
