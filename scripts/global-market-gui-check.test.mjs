// @vitest-environment node
import { describe, expect, it } from "vitest";
import { judgeInteraction } from "./global-market-gui-check.mjs";

const seen = {
  error: undefined,
  caption: "Global market, page 1 of 2",
  rows: 25,
  focusedProfile: true,
  selectedRow: true,
  outcome: "Agreed",
  signEnabled: true,
  dialogOpened: true,
  dialogClosed: true,
  successNotice: true,
  focusAfterSigning: true,
  pageAfterNext: "Page 2 of 2",
  pageAfterPrevious: "Page 1 of 2",
  paginationFocus: true,
  smallText: [],
  scrollWidth: 1280,
  innerWidth: 1280,
};

describe("the Global Market interaction check", () => {
  /** Given every step behaved, then there is nothing to report. */
  it("passes a run where selection, signing and paging behaved", () => {
    expect(judgeInteraction(seen)).toEqual([]);
  });

  /** Given focus stayed behind after a selection, then that step is named. */
  it("names a selection that did not take focus", () => {
    expect(judgeInteraction({ ...seen, focusedProfile: false })).toEqual([
      "selecting a player did not move focus to the profile",
    ]);
  });

  /** Given a successful signing that left focus elsewhere, then that step is named. */
  it("names a success message that did not take focus", () => {
    expect(judgeInteraction({ ...seen, focusAfterSigning: false })).toEqual([
      "signing did not move focus to the success message",
    ]);
  });

  /** Given paging left focus on a disabled control, then that step is named. */
  it("names paging that kept focus on a disabled control", () => {
    expect(judgeInteraction({ ...seen, paginationFocus: false })).toEqual([
      "paging back did not move focus off the now-disabled Previous page",
    ]);
  });

  /** Given text under 12px or a sideways scroll, then both are reported. */
  it("reports small text and a sideways scroll", () => {
    expect(judgeInteraction({ ...seen, smallText: ["span tiny"], scrollWidth: 1300 })).toEqual([
      "text under 12px: span tiny",
      "the page scrolls sideways (1300 > 1280)",
    ]);
  });

  /** Given a page that never rendered a row, then the run stops with that one reason. */
  it("stops when the market never rendered", () => {
    expect(judgeInteraction({ ...seen, error: "the market never rendered a row" })).toEqual([
      "the market never rendered a row",
    ]);
  });
});
