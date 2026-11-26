// Ranking for the command palette (cmdk `filter`).
//
// Items pass `keywords = [label, ...extras]`, where `label` is the visible
// text. A query that is a prefix / word start / substring of the label always
// outranks scattered-character matches, so a partial name runs what the user
// meant ("notes" → notes.md, "again" → "Choose reading aids again"). Items
// marked STRICT (state-changing widget actions: pin / hide / show) never match
// on scattered characters at all.

export const STRICT = "\u0000strict";

const norm = (s: string) => s.normalize("NFKC").toLowerCase().trim();

function subsequence(q: string, t: string): boolean {
  let i = 0;
  for (const ch of t) if (ch === q[i]) i++;
  return i >= q.length;
}

export function paletteFilter(value: string, search: string, keywords?: string[]): number {
  const q = norm(search);
  if (!q) return 1;
  const kws = keywords ?? [];
  const strict = kws.includes(STRICT);
  const label = norm(kws[0] ?? value);
  const extras = kws.slice(1).filter((k) => k !== STRICT).map(norm);
  if (label.startsWith(q)) return 1;
  if (new RegExp(`(^|[\\s/._\\-()])${q.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}`).test(label)) return 0.9;
  if (label.includes(q)) return 0.8;
  if (extras.some((e) => e.startsWith(q))) return 0.6;
  if (extras.some((e) => e.includes(q))) return 0.5;
  if (strict) return 0;
  // Scattered characters: last resort, always below any substring match.
  if (subsequence(q, label)) return 0.1 * (q.length / Math.max(label.length, 1));
  return 0;
}
