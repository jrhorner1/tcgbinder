// Moving between pages. The address's hash names the page and its parameters,
// e.g. #/collection?set=m19; `route` follows it, and App.svelte shows its page.

function read() {
  const [path, qs] = location.hash.replace(/^#\/?/, "").split("?");
  return { view: path || "home", params: new URLSearchParams(qs || "") };
}

export const route = $state(read());

addEventListener("hashchange", () => Object.assign(route, read()));

/** Goes to a page. `replace` changes the address without a new history entry (for
 *  filters typed letter by letter). */
export function go(view: string, params?: URLSearchParams | Record<string, string>, replace = false) {
  const p = params instanceof URLSearchParams ? params : new URLSearchParams(params || {});
  const qs = [...p.keys()].length ? `?${p}` : "";
  const hash = `#/${view}${qs}`;
  if (replace || location.hash === hash) {
    history.replaceState(null, "", hash);
    Object.assign(route, read());
  } else {
    location.hash = hash;
  }
}

/** The current page's parameters, copied, to change and pass to go(). */
export const params = () => new URLSearchParams(route.params);
