// The UI's shared state: the app's info and settings, the game shown, its filter
// choices, ticked cards and the like. Every file reads and changes this one object.

export const state = {
  info: null, settings: null, game: null, filters: null, range: null,
  // Ticked cards for bulk edit, per game: key → {name, where}. Ticks stay while the
  // filters hide the cards, so a selection can be built from several searches.
  ticked: new Map(),
  revision: null, skipRevision: false,
  scanStop: null, // stops the camera when leaving the Scan page
};
