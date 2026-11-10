import { Columns3 } from "lucide-react";
import { useEffect, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import { Button, Checkbox } from "../ui";
import { HIDEABLE_SQUAD_COLUMNS, type HideableSquadColumnId } from "./squadColumns";

interface SquadColumnChooserProps {
  hidden: ReadonlySet<HideableSquadColumnId>;
  onToggle: (id: HideableSquadColumnId) => void;
  onShowAll: () => void;
}

/**
 * Which optional columns of the squad table show. A disclosure, not a modal: the button says
 * whether the panel is open, the boxes are real checkboxes in the tab order, and Escape closes the
 * panel and puts focus back on the button. The player's name has no box because it is never hidden.
 */
export function SquadColumnChooser({ hidden, onToggle, onShowAll }: SquadColumnChooserProps) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const panelId = useId();
  const toggle = useRef<HTMLButtonElement>(null);
  const wrapper = useRef<HTMLDivElement>(null);

  // Escape closes the panel when focus is inside it (or on its button), and hands focus back to the
  // button. Escape pressed in a menu or dialog elsewhere is that widget's, not this panel's.
  useEffect(() => {
    if (!open) return;
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || event.defaultPrevented) return;
      if (!wrapper.current?.contains(document.activeElement)) return;
      setOpen(false);
      toggle.current?.focus();
    };
    document.addEventListener("keydown", closeOnEscape);
    return () => document.removeEventListener("keydown", closeOnEscape);
  }, [open]);

  return (
    <div ref={wrapper} className="relative">
      <button
        ref={toggle}
        type="button"
        aria-expanded={open}
        aria-controls={open ? panelId : undefined}
        onClick={() => setOpen((current) => !current)}
        className="inline-flex items-center gap-1.5 rounded-lg border border-line bg-panel px-3 py-2 font-heading text-caption font-bold uppercase tracking-label text-fg-muted transition-colors hover:text-fg focus:outline-none focus-visible:ring-2 focus-visible:ring-focus focus-visible:ring-offset-2 focus-visible:ring-offset-page"
      >
        <Columns3 aria-hidden="true" className="h-4 w-4" />
        {t("squad.columnsChoose")}
      </button>
      {open ? (
        <fieldset
          id={panelId}
          className="absolute right-0 z-20 mt-1 grid min-w-48 gap-2 rounded-lg border border-line bg-panel p-3 shadow-lg"
        >
          <legend className="sr-only">{t("squad.columnsChoose")}</legend>
          {HIDEABLE_SQUAD_COLUMNS.map(({ id, labelKey }) => (
            <div key={id} className="flex items-center gap-2 text-body text-fg">
              <Checkbox
                id={`${panelId}-${id}`}
                checked={!hidden.has(id)}
                aria-label={t(labelKey)}
                onChange={() => onToggle(id)}
              />
              <label htmlFor={`${panelId}-${id}`}>{t(labelKey)}</label>
            </div>
          ))}
          <Button size="sm" variant="ghost" onClick={onShowAll}>
            {t("squad.columnsShowAll")}
          </Button>
        </fieldset>
      ) : null}
    </div>
  );
}
