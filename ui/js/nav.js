// Moving between pages: go() changes the address, and route (set by main.js at
// start-up) draws the page for it. Pages import route from here to redraw, so nothing
// imports main.js.

export const hashParams = () => new URLSearchParams(location.hash.split("?")[1]);

// The router, given by main.js at start-up (see setRoute). Pages call route() to
// redraw the page shown.
export let route = () => {};
export function setRoute(fn) { route = fn; }

export function go(view, params, replace) {
  const qs = params && [...params.keys()].length ? `?${params}` : "";
  const hash = `#/${view}${qs}`;
  // Setting the same hash fires no hashchange (e.g. reopening a card after saving it).
  if (replace || location.hash === hash) { history.replaceState(null, "", hash); route(); } else location.hash = hash;
}
