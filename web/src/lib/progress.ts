// Reader-side progress for operable widgets (ticks, current step).
//
// Stored per document in the persisted prefs (localStorage). It never touches
// the Markdown source: the file stays the single source of truth, and a reset
// returns every item to what the file says.
import { createContext, useContext, useState } from "react";
import { docPrefs, updateDocPrefs, useStore } from "./store";

/** The widget key of the margin note a component is rendered in (null = not in a note). */
export const WidgetKeyCtx = createContext<string | null>(null);

/** One tick-able item: initial value from the source, override kept per document. */
export function useMark(itemText: string, fromSource: boolean): [boolean, () => void] {
  const wk = useContext(WidgetKeyCtx);
  const key = wk ? `${wk}|${itemText}` : null;
  const stored = useStore((s) => (key ? docPrefs(s).marks?.[key] : undefined));
  const [local, setLocal] = useState<boolean | undefined>(undefined);
  const value = key ? (stored ?? fromSource) : (local ?? fromSource);
  const toggle = () => {
    if (!key) return setLocal(!value);
    updateDocPrefs((d) => {
      const marks = { ...(d.marks ?? {}) };
      if (!value === fromSource) delete marks[key];
      else marks[key] = !value;
      return { ...d, marks };
    });
  };
  return [value, toggle];
}

/** Whether any item of this widget differs from the file, and a reset for it. */
export function useWidgetMarks(): { changed: boolean; reset: () => void } {
  const wk = useContext(WidgetKeyCtx);
  const changed = useStore((s) => !!wk && Object.keys(docPrefs(s).marks ?? {}).some((k) => k.startsWith(`${wk}|`)));
  const reset = () => {
    if (!wk) return;
    updateDocPrefs((d) => {
      const marks = Object.fromEntries(Object.entries(d.marks ?? {}).filter(([k]) => !k.startsWith(`${wk}|`)));
      return { ...d, marks };
    });
  };
  return { changed, reset };
}

/** Current step of a StepNavigator (clamped to the step count). */
export function useStep(count: number): [number, (i: number) => void] {
  const wk = useContext(WidgetKeyCtx);
  const stored = useStore((s) => (wk ? docPrefs(s).steps?.[wk] : undefined));
  const [local, setLocal] = useState(0);
  const raw = wk ? (stored ?? 0) : local;
  const cur = Math.max(0, Math.min(count - 1, raw));
  const set = (i: number) => {
    const n = Math.max(0, Math.min(count - 1, i));
    if (!wk) return setLocal(n);
    updateDocPrefs((d) => ({ ...d, steps: { ...(d.steps ?? {}), [wk]: n } }));
  };
  return [cur, set];
}

/** Copy text to the clipboard (localhost is a secure context); falls back to execCommand. */
export async function copyText(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    try {
      const ta = document.createElement("textarea");
      ta.value = text;
      ta.setAttribute("readonly", "");
      ta.style.position = "fixed";
      ta.style.opacity = "0";
      document.body.appendChild(ta);
      ta.select();
      const ok = document.execCommand("copy");
      ta.remove();
      return ok;
    } catch {
      return false;
    }
  }
}
