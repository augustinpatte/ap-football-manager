#!/usr/bin/env node
// Drives the Global Market in headless Chrome through the dev page harness: select a player,
// negotiate, confirm a signing, page back and forth, and read back what the keyboard and the
// screen actually did. This is browser/harness evidence, not a Tauri desktop run.
//
//   npm run dev                              # in another terminal
//   node scripts/global-market-gui-check.mjs [http://localhost:1420]
//
// Exits 1 when a check fails. Needs Node 22+ for the built-in WebSocket and an installed Chrome
// (or CHROME_PATH). It is not a CI gate: it needs a running dev server and a browser.

import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { connect, devtools, launchChrome, nodeProblem, stopChrome } from "./lib/chrome-cdp.mjs";

/** What the page did, as read back from the DOM; empty fields mean the step never happened. */
export function judgeInteraction(seen) {
  const problems = [];
  if (seen.error) return [seen.error];
  if (!seen.caption) problems.push("the results table has no caption");
  if (seen.rows < 1) problems.push("the results table has no rows");
  if (!seen.focusedProfile) problems.push("selecting a player did not move focus to the profile");
  if (!seen.selectedRow) problems.push("the selected row is not exposed with aria-current");
  if (!seen.outcome) problems.push("checking the terms never showed an agreed outcome");
  if (!seen.signEnabled) problems.push("the sign button stayed disabled after an agreement");
  if (!seen.dialogOpened) problems.push("signing did not open the confirmation dialog");
  if (!seen.dialogClosed) problems.push("the confirmation dialog stayed open after signing");
  if (!seen.successNotice) problems.push("signing never showed the success message");
  if (!seen.focusAfterSigning) problems.push("signing did not move focus to the success message");
  if (!seen.pageAfterNext) problems.push("Next page did not reach page 2");
  if (!seen.pageAfterPrevious) problems.push("Previous page did not return to page 1");
  if (!seen.paginationFocus) {
    problems.push("paging back did not move focus off the now-disabled Previous page");
  }
  if (seen.smallText.length > 0) {
    problems.push(`text under 12px: ${seen.smallText.join(", ")}`);
  }
  if (seen.scrollWidth > seen.innerWidth) {
    problems.push(`the page scrolls sideways (${seen.scrollWidth} > ${seen.innerWidth})`);
  }
  return problems;
}

const DRIVE = String.raw`(async () => {
  const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
  const waitFor = async (read, timeout = 8000) => {
    const start = Date.now();
    while (Date.now() - start < timeout) {
      const value = read();
      if (value) return value;
      await sleep(50);
    }
    return null;
  };
  const textOf = (element) => (element?.textContent ?? "").trim();
  const button = (label) =>
    [...document.querySelectorAll("button")].find((element) => textOf(element) === label);
  const statusWith = (fragment) =>
    [...document.querySelectorAll('[role="status"]')].find((element) =>
      textOf(element).includes(fragment),
    );
  const pageLabel = () =>
    [...document.querySelectorAll("span")]
      .map(textOf)
      .find((text) => /^Page \d+ of \d+$/.test(text)) ?? "";
  const seen = { smallText: [], scrollWidth: 0, innerWidth: window.innerWidth };

  const firstRow = await waitFor(() => document.querySelector("table tbody tr"));
  if (!firstRow) return JSON.stringify({ ...seen, error: "the market never rendered a row" });

  seen.rows = document.querySelectorAll("table tbody tr").length;
  seen.caption = textOf(document.querySelector("table caption"));

  firstRow.querySelector("button")?.click();
  const profile = await waitFor(() =>
    document.querySelector('[role="region"][aria-label="Player profile"]'),
  );
  await waitFor(() => button("Check the terms"));
  await sleep(300);
  seen.focusedProfile = document.activeElement === profile;
  seen.selectedRow = document.querySelector('table tbody tr[aria-current="true"]') !== null;

  button("Check the terms")?.click();
  seen.outcome = (await waitFor(() => statusWith("Agreed"))) ? "Agreed" : "";
  const sign = button("Sign player");
  seen.signEnabled = Boolean(sign && !sign.disabled);

  sign?.click();
  const dialog = await waitFor(() => document.querySelector('[role="dialog"]'));
  seen.dialogOpened = Boolean(dialog);
  seen.dialogTrap = Boolean(dialog && dialog.contains(document.activeElement));

  [...(dialog?.querySelectorAll("button") ?? [])]
    .find((element) => textOf(element) === "Confirm signing")
    ?.click();
  const notice = await waitFor(() => statusWith("has joined"));
  seen.successNotice = Boolean(notice);
  seen.focusAfterSigning = Boolean(notice && document.activeElement === notice);
  seen.dialogClosed = document.querySelector('[role="dialog"]') === null;

  document.querySelector('button[aria-label="Next page"]')?.click();
  seen.pageAfterNext = (await waitFor(() => pageLabel() === "Page 2 of 2")) ? pageLabel() : "";
  const previous = await waitFor(() => {
    const control = document.querySelector('button[aria-label="Previous page"]');
    return control && !control.disabled ? control : null;
  });
  previous?.click();
  seen.pageAfterPrevious = (await waitFor(() => pageLabel() === "Page 1 of 2")) ? pageLabel() : "";
  await sleep(250);
  seen.paginationFocus =
    document.activeElement === document.querySelector('button[aria-label="Next page"]');

  const visibleText = (element) =>
    [...element.childNodes].some((node) => node.nodeType === Node.TEXT_NODE && node.textContent.trim());
  seen.smallText = [...document.querySelectorAll("body *")]
    .filter((element) => {
      if (!visibleText(element)) return false;
      const rect = element.getBoundingClientRect();
      return rect.width > 4 && rect.height > 4 && parseFloat(getComputedStyle(element).fontSize) < 12;
    })
    .slice(0, 10)
    .map((element) => element.tagName.toLowerCase() + " " + textOf(element).slice(0, 24));

  seen.scrollWidth = document.documentElement.scrollWidth;
  return JSON.stringify(seen);
})()`;

async function evaluate(call, expression) {
  const reply = await call("Runtime.evaluate", {
    expression,
    awaitPromise: true,
    returnByValue: true,
  });
  if (reply.result?.exceptionDetails) {
    throw new Error(reply.result.exceptionDetails.exception?.description ?? "the page threw");
  }
  return reply.result.result.value;
}

async function main(base) {
  const profile = mkdtempSync(join(tmpdir(), "global-market-gui-check-"));
  const chrome = launchChrome(profile);
  const exited = new Promise((resolve) => chrome.once("exit", resolve));
  try {
    const { socket, call } = await connect(await devtools(profile, chrome));
    await call("Emulation.setDeviceMetricsOverride", {
      width: 1280,
      height: 800,
      deviceScaleFactor: 1,
      mobile: false,
    });
    await call("Page.navigate", {
      url: `${base}/style-guide.html?theme=light&scale=normal&lang=en&page=globalMarket`,
    });
    await new Promise((resolve) => setTimeout(resolve, 2500));
    const seen = JSON.parse(await evaluate(call, DRIVE));
    const problems = judgeInteraction(seen);
    console.log(
      problems.length === 0
        ? `ok   Global Market: ${seen.rows} rows, signing, paging and focus behave`
        : `FAIL Global Market: ${problems.join("; ")}`,
    );
    socket.close();
    process.exitCode = problems.length > 0 ? 1 : 0;
  } finally {
    if (chrome.pid !== undefined && chrome.exitCode === null) {
      stopChrome(chrome);
      await exited;
    }
    rmSync(profile, { recursive: true, force: true, maxRetries: 10, retryDelay: 200 });
  }
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) {
  const problem = nodeProblem(globalThis.WebSocket, process.version);
  if (problem) {
    console.error(problem);
    process.exit(1);
  }
  await main(process.argv[2] ?? "http://localhost:1420");
}
