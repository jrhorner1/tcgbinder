// What the app's JSON API sends (docs/web-api.md), as the UI uses it. Fields the UI
// doesn't read are left out; add them here when a page needs one.

export interface GameInfo { ID: string; Name: string; Scan: boolean }

export interface Info {
  name: string;
  version: string;
  games: GameInfo[];
  client: "desktop" | "phone" | "web";
  saveExports: boolean;
  opensLinks: boolean;
  /** The value chart's ranges, e.g. 30D, 90D. */
  ranges: string[];
}

export interface Settings {
  theme: "system" | "light" | "dark";
  text_size: "small" | "normal" | "large" | "larger";
  show_images: boolean;
  confirm_delete: boolean;
  card_order: string;
  set_completion: string;
  chart_range: string;
  price_refresh: "daily" | "weekly" | "manual";
  default_condition: string;
  default_date: "today" | "none";
  log_level: string;
  [other: string]: unknown;
}

export interface GameFilter { Name: string; Options: string[] }

export interface Filters {
  sets: { Code: string; Label: string }[];
  rarities: string[];
  finishes: string[];
  finishNames: Record<string, string>;
  filters: GameFilter[];
  conditions: string[];
  languages: { code: string; name: string }[];
  tags: string[];
}

/** One purchase of a card: copies alike in everything but how many. */
export interface Purchase {
  finish: string;
  condition: string;
  quantity: number;
  price_paid: number;
  acquired_date: string;
  /** The lot (purchase) it was bought in: a lot's ID, or "". */
  lot: string;
  tags: string[];
  /** One copy's market price in its finish (0 when unpriced). */
  market: number;
}

/** A catalog printing: a search result, or the card part of an owned card. */
export interface CatalogCard {
  id: string;
  name: string;
  set: string;
  set_name: string;
  collector_number: string;
  rarity: string;
  released_at: string;
  image_uri: string;
  type_line?: string;
  oracle_text?: string;
  lang: string;
  finishes: string[];
  market?: Record<string, number>;
  price_from?: string;
  each?: number;
}

/** A card you own, as GET /cards and GET /card give it. */
export interface Card extends CatalogCard {
  key: string;
  copy_count: number;
  each: number;
  market_value: number;
  price_paid: number;
  purchases: Purchase[];
}

export interface CardList {
  cards: Card[];
  matched: number;
  uniqueCards: number;
  cardCount: number;
  sets: number;
  marketValue: number;
  pricePaid: number;
  sort: string;
  dir: "asc" | "desc";
}

/** A lot: a bundle, box, eBay lot or other purchase of many cards at one price. */
export interface Lot {
  id: string;
  name: string;
  acquired_date: string;
  total_paid: number;
  count: { kind: "known"; cards: number } | { kind: "bundle"; packs: number; cards_per_pack: number; extra_cards: number; lands: number; lands_share_cost: boolean } | { kind: "as_entered" };
  split: "by_value" | "even";
  note: string;
  open: boolean;
  created: string;
  split_date: string;
  entered: number;
  copies: number;
  cards: number;
  expected: number | null;
  priceEach: number | null;
  marketValue: number;
}

/** A change the server backed up first: sending it to backups/restore undoes it. */
export interface Undo { backup: string; revision: number }

export interface Job { name: string; done: number; total: number; message: string }

export interface ListSummary {
  id: string; name: string; notes: string; created: string; remove_owned: boolean;
  entries: number; cards: number; owned: number; missing: number; marketValue: number; missingCost: number;
}

export interface TagTotal { name: string; cards: number; copies: number; value: number; paid: number }
