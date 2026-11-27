// Tags: the rules for one (as crates/core/src/collection/tags.rs has them), comparing
// and sorting them, and the color each tag always gets.

import type { Card } from "./types";

export const TAG_MAX_LEN = 40;
export const TAGS_MAX = 20;

export const tidyTag = (raw: string) => raw.trim().split(/\s+/).filter(Boolean).join(" ");

export const sameTag = (a: string, b: string) => a.toLowerCase() === b.toLowerCase();

export const sortTags = (tags: string[]) => tags.sort((a, b) => a.toLowerCase().localeCompare(b.toLowerCase()));

/** Why `raw` can't be a tag, or "" if it can. */
export function tagProblem(raw: string) {
  const clean = tidyTag(raw);
  // eslint-disable-next-line no-control-regex
  if (/[\u0000-\u001f\u007f-\u009f]/.test(raw)) return "A tag can't contain control characters.";
  if (!clean) return "A tag can't be empty.";
  if ([...clean].length > TAG_MAX_LEN) return `A tag can be at most ${TAG_MAX_LEN} characters.`;
  if (/[,;]/.test(clean)) return "A tag can't contain a comma or a semicolon.";
  return "";
}

/** A card's tags: every tag on any of its purchases. */
export const cardTags = (c: Card) => sortTags([...new Map(c.purchases.flatMap((p) => p.tags).map((t) => [t.toLowerCase(), t])).values()]);

/** A hue from a tag's name, so the same tag always looks the same. */
export function tagHue(name: string) {
  let hash = 0;
  for (const ch of name.toLowerCase()) hash = (hash * 31 + (ch.codePointAt(0) || 0)) >>> 0;
  return hash % 360;
}
