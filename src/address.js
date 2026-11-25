// What the address bar does with what was typed into it.
//
// One place, because two surfaces ask the question - the chrome strip and the new tab
// page - and they used to answer it differently: the strip turned a search into a search
// engine url, while the new tab page handed the raw text to `tab_navigate`, which rejects
// anything that is not an address. Typing a question there replaced the tab with the
// "not a web address" error page.
//
// `settings` is passed in rather than read here: both callers already have it, and this
// file stays a pure decision about the text.

export const SEARCH_ENGINES = {
  baidu: "https://www.baidu.com/s?wd=",
  bing: "https://www.bing.com/search?q=",
  google: "https://www.google.com/search?q=",
  ddg: "https://duckduckgo.com/?q=",
};

/** The app's own pages, by the short name the address bar shows. */
export function internalPage(name) {
  return "http://tauri.localhost/pages/" + name + ".html";
}

/**
 * The url this text asks for, when it is an address.
 *
 * Null means "this is not an address", which is the signal to search instead. Deliberately
 * forgiving about a bare host (`example.com`) and deliberately strict about anything with a
 * space in it, because `rust async` is a question and not a typo'd domain.
 */
export function looksLikeAddress(text) {
  const value = (text || "").trim();
  if (!value) return null;
  // Before the scheme test: `mix://` is itself a well formed scheme, so the generic branch
  // used to swallow it and navigate to an address no webview can open. The app's own names
  // are the one case that has to be translated into the app's own origin.
  if (value.startsWith("mix://")) return internalPage(value.slice(6).split("/")[0]);
  if (/^[a-z][a-z0-9+.-]*:\/\//i.test(value)) return value;
  if (value.includes(" ")) return null;

  // Local and intranet addresses before the dotted-name rule, which would otherwise claim
  // an IPv4 literal and force https on it. Everything here is http: these are machines the
  // reader runs, and a dev server has no certificate.
  const local = looksLikeLocalAddress(value);
  if (local) return local;

  if (!value.includes(".")) return null;
  return "https://" + value;
}

/**
 * A host (and optional port) that no public web site would use, as an http url.
 *
 * The cases this exists for are all local or intranet: `localhost`, an IPv4 literal, or a
 * bare name with a port on it. Each of them was previously sent to the search engine, so
 * typing an address the reader runs themselves opened a results page instead.
 *
 * Only the authority is judged - the path, query and fragment are put back unchanged, so
 * `localhost:3000/api?x=1` keeps its way of addressing the request.
 */
function looksLikeLocalAddress(value) {
  const cut = value.search(/[/?#]/);
  const authority = cut < 0 ? value : value.slice(0, cut);
  const rest = cut < 0 ? "" : value.slice(cut);
  if (!authority) return null;

  // An IPv4 literal, with or without a port. Checked first because it contains dots and
  // would otherwise be taken for a hostname and forced onto https.
  const ipv4 = /^(\d{1,3}(?:\.\d{1,3}){3})(?::(\d{1,5}))?$/.exec(authority);
  if (ipv4 && portOk(ipv4[2])) return "http://" + authority + rest;
  if (authority === "localhost") return "http://localhost" + rest;
  // host:port, where the host is a single label, a dotted name or a bracketed IPv6 literal.
  // The port is what makes a bare word unambiguous: `intranet` on its own is a search,
  // `intranet:8080` is an address.
  const ported = /^([a-z0-9][a-z0-9.-]*|\[[0-9a-f:]+\]):(\d{1,5})$/i.exec(authority);
  if (ported && portOk(ported[2])) return "http://" + authority + rest;
  return null;
}

/** A port the networking layer would accept, so a typo is a search and not a bad url. */
function portOk(text) {
  if (text === undefined) return true;
  const port = Number(text);
  return port > 0 && port <= 65535;
}

/**
 * Where to send the reader, whether they typed an address or a question.
 *
 * `custom_search` wins when it holds a `%s`, which is what the row promises: a template
 * the reader supplied is more specific than the engine they picked from a list.
 */
export function addressFor(text, settings) {
  const direct = looksLikeAddress(text);
  if (direct) return direct;
  const query = (text || "").trim();
  const template = ((settings && settings.custom_search) || "").trim();
  if (template.includes("%s")) {
    return template.replace("%s", encodeURIComponent(query));
  }
  const engine = (settings && settings.search_engine) || "baidu";
  const base = SEARCH_ENGINES[engine] || SEARCH_ENGINES.baidu;
  return base + encodeURIComponent(query);
}
