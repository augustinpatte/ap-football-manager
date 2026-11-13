import { describe, expect, it } from "vitest";

import {
  HIDEABLE_SQUAD_COLUMNS,
  hiddenSquadColumns,
  sortKeyAfterHiding,
  toggleHiddenSquadColumn,
} from "./squadColumns";

describe("the squad table's optional columns", () => {
  /** Given nothing stored, then no column is hidden: the table is as it always was. */
  it("hides nothing by default", () => {
    expect(hiddenSquadColumns([]).size).toBe(0);
  });

  /** Given the player's name, then it can never be hidden, whatever a settings file says. */
  it("never lists the player's name as hideable, even if a file says to hide it", () => {
    expect(HIDEABLE_SQUAD_COLUMNS.map((column) => column.id)).not.toContain("name");
    expect(hiddenSquadColumns(["name", "age"])).toEqual(new Set(["age"]));
  });

  /** Given ids this build does not know (a newer build's), then they hide nothing here. */
  it("ignores ids it does not know", () => {
    expect(hiddenSquadColumns(["from-the-future", "morale"])).toEqual(new Set(["morale"]));
  });

  /** Given a visible column, when it is toggled twice, then it is hidden and then shown again. */
  it("hides and shows a column on toggle, keeping the order they were hidden in", () => {
    const hidden = toggleHiddenSquadColumn(toggleHiddenSquadColumn([], "age"), "morale");
    expect(hidden).toEqual(["age", "morale"]);
    expect(toggleHiddenSquadColumn(hidden, "age")).toEqual(["morale"]);
  });

  /** Given an id this build does not know, when another column is toggled, then it is kept. */
  it("keeps what it does not know when it saves a change", () => {
    expect(toggleHiddenSquadColumn(["from-the-future"], "age")).toEqual(["from-the-future", "age"]);
  });

  /** Given the name, when it is asked to hide, then nothing changes. */
  it("refuses to hide the name", () => {
    expect(toggleHiddenSquadColumn(["age"], "name")).toEqual(["age"]);
  });

  /**
   * Given the table is sorted by a column that is then hidden,
   * then the sort falls back to the default, so the order on screen is never set by a column the
   * player cannot see; a visible sort column is left alone.
   */
  it("drops a sort on a column that has been hidden", () => {
    expect(sortKeyAfterHiding("age", new Set(["age"]))).toBe("pos");
    expect(sortKeyAfterHiding("ovr", new Set(["age"]))).toBe("ovr");
    expect(sortKeyAfterHiding("name", new Set(["age"]))).toBe("name");
  });
});
