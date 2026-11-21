import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import PlayerProfileRenewalModal from "./PlayerProfileRenewalModal";
import PlayerProfileTerminationModal from "./PlayerProfileTerminationModal";

vi.mock("react-i18next", () => ({
  useTranslation: () => ({ t: (key: string) => key, i18n: { language: "en" } }),
}));

const t = (key: string): string => key;

function renewal(overrides: Partial<React.ComponentProps<typeof PlayerProfileRenewalModal>> = {}) {
  const onClose = vi.fn();
  render(
    <PlayerProfileRenewalModal
      show
      playerName="John Smith"
      t={t}
      renewalWage="5000"
      renewalLength="2"
      renewalIsTerminal={false}
      isRenewalWageValid
      renewalViolatesSoftCap={false}
      renewalProjection={null}
      renewalStatusMessage={null}
      renewalStatusClassName=""
      renewalCooledOff={false}
      renewalFeedback={null}
      renewalSubmitDisabled={false}
      delegateRenewalDisabled={false}
      onWageChange={vi.fn()}
      onLengthChange={vi.fn()}
      onClose={onClose}
      onDelegate={vi.fn()}
      onSubmit={vi.fn()}
      {...overrides}
    />,
  );
  return { onClose };
}

describe("contract dialogs", () => {
  /**
   * Given the renewal offer is open
   * When assistive technology and the keyboard reach it
   * Then it is a modal dialog named by its heading, bound to the window, and Escape closes it.
   */
  it("makes the renewal offer a named, window-bound, keyboard-closable dialog", () => {
    const { onClose } = renewal();

    const dialog = screen.getByRole("dialog", { name: "playerProfile.renewalTitle" });
    expect(dialog.className).toContain("max-h-full");
    expect(dialog.className).toContain("overflow-y-auto");

    fireEvent.keyDown(dialog, { key: "Escape" });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  /**
   * Given a renewal offer tall enough to scroll
   * When it is open
   * Then Cancel, Delegate and Submit sit in the pinned footer, not at the end of the content.
   */
  it("pins the renewal actions to the bottom of the dialog", () => {
    renewal();

    const submit = screen.getByRole("button", { name: "playerProfile.renewalSubmit" });
    expect(submit.parentElement?.className).toContain("sticky");
    expect(submit.parentElement?.className).toContain("bottom-0");
  });

  /**
   * Given the termination confirmation is open
   * When it is reached by assistive technology
   * Then it is a modal dialog named by its heading, and Escape cancels it.
   */
  it("makes the termination confirmation a named dialog that Escape cancels", () => {
    const onCancel = vi.fn();
    render(
      <PlayerProfileTerminationModal
        show
        playerName="John Smith"
        t={t}
        preview={null}
        errorMessage={null}
        submitting={false}
        onCancel={onCancel}
        onConfirm={vi.fn()}
      />,
    );

    const dialog = screen.getByRole("dialog", { name: "playerProfile.terminateContractTitle" });
    fireEvent.keyDown(dialog, { key: "Escape" });
    expect(onCancel).toHaveBeenCalledTimes(1);
  });

  /**
   * Given the release is being submitted
   * When Escape is pressed
   * Then the dialog does not cancel mid-submission.
   */
  it("does not cancel the termination while it is being submitted", () => {
    const onCancel = vi.fn();
    render(
      <PlayerProfileTerminationModal
        show
        playerName="John Smith"
        t={t}
        preview={null}
        errorMessage={null}
        submitting
        onCancel={onCancel}
        onConfirm={vi.fn()}
      />,
    );

    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    expect(onCancel).not.toHaveBeenCalled();
  });
});
