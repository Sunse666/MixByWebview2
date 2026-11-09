// Builds every internal page under a stub DOM, so a page that throws while
// wiring itself up is caught before shipping instead of showing a blank page.
//
//   node scripts/check-pages.js            # every page
//   node scripts/check-pages.js settings   # one page

const path = require("path");
const { pathToFileURL } = require("url");

const PAGES = [
  "settings",
  "history",
  "bookmarks",
  "downloads",
  "extensions",
  "scripts",
  "newtab",
  "sidebar",
];

class El {
  constructor(tag) {
    this.tagName = String(tag).toUpperCase();
    this.children = [];
    this.style = new Proxy({ setProperty() {} }, { get: (t, k) => (k in t ? t[k] : ""), set: (t, k, v) => { t[k] = v; return true; } });
    this.dataset = {};
    this._attrs = {};
    this.className = "";
    this.textContent = "";
    this.value = "";
    this.hidden = false;
    this.checked = false;
    this.disabled = false;
    this.classList = {
      add: (...c) => { for (const x of c) if (!this.className.split(" ").includes(x)) this.className += " " + x; },
      remove: (...c) => { this.className = this.className.split(" ").filter((y) => !c.includes(y)).join(" "); },
      toggle: (c, on) => { const want = on === undefined ? !this.classList.contains(c) : on; if (want) this.classList.add(c); else this.classList.remove(c); },
      contains: (c) => this.className.split(" ").includes(c),
    };
  }
  // Real DOM append moves an existing child instead of duplicating it.
  append(...nodes) {
    for (const n of nodes) {
      if (n === null || n === undefined) continue;
      const at = this.children.indexOf(n);
      if (at >= 0) this.children.splice(at, 1);
      this.children.push(n);
    }
  }
  appendChild(n) { this.append(n); return n; }
  insertBefore(n, ref) {
    const at = this.children.indexOf(ref);
    if (at < 0) this.append(n);
    else { this.children.splice(at, 0, n); }
    return n;
  }
  focus() {}
  blur() {}
  select() {}
  setSelectionRange() {}
  scrollTo() {}
  closest() { return null; }
  click() {}
  replaceChildren(...n) { this.children = n.filter(Boolean); }
  removeChild(n) { this.children = this.children.filter((c) => c !== n); return n; }
  addEventListener() {}
  removeEventListener() {}
  setAttribute(k, v) { this._attrs[k] = String(v); }
  getAttribute(k) { return this._attrs[k]; }
  querySelectorAll() { return []; }
  querySelector() { return null; }
  remove() {}
  scrollIntoView() {}
  get firstChild() { return this.children[0] || null; }
}

const SETTINGS = {
  language: "en", start_url: "https://example.com", home_url: "", restore_session: true,
  show_bookmarks: false, toolbar: ["translate", "zoom", "bookmark", "extensions", "downloads"],
  font_family: "", font_mono: "", font_size: 0, font_min_size: 0, default_zoom: 1,
  search_engine: "baidu", custom_search: "", theme_mode: "dark", memory_saver: true,
  max_live_tabs: 12, keep_alive_hosts: "", mark_suspended: true, gestures: true,
  gesture_trail: true, gesture_color: "#4c8dff", gesture_width: 3, gesture_threshold: 34,
  gesture_map: {}, gesture_blocked: "", super_drag: true, drag_hint: true, drag_threshold: 28,
  drag_map: {}, tab_double_click_close: true, tab_show_close: true, tab_close_goes_left: true,
  newtab_show_search: true, newtab_show_sites: true, newtab_url: "", newtab_tiles: 12,
  newtab_columns: 6, newtab_radius: 50, shortcuts: {}, hidden_extensions: [], disabled_extensions: [],
};

function installDom() {
  const roots = {};
  const getRoot = (id) => (roots[id] = roots[id] || new El("div"));
  const document = {
    createElement: (tag) => new El(tag),
    getElementById: (id) => getRoot(id),
    querySelectorAll: () => [],
    querySelector: () => null,
    addEventListener() {},
    body: new El("body"),
    documentElement: new El("html"),
    title: "",
  };
  const invoke = async (command) => {
    if (command === "settings_get") return SETTINGS;
    if (command === "theme_get") return { colors: {}, radius: 15, tab_radius: 8, font_size: 13, font_family: "" };
    if (command === "favicon_map") return {};
    if (command.endsWith("_list")) return [];
    return null;
  };
  globalThis.document = document;
  globalThis.location = { hash: "", origin: "http://tauri.localhost", reload() {} };
  globalThis.matchMedia = () => ({ matches: false, addEventListener() {} });
  globalThis.window = globalThis;
  // Pages register window listeners; the stub only has to accept them.
  globalThis.addEventListener = () => {};
  globalThis.removeEventListener = () => {};
  globalThis.__TAURI__ = { core: { invoke }, event: { listen: async () => () => {} } };
  return roots;
}

let pageErrors = [];
process.on("unhandledRejection", (error) => {
  pageErrors.push(error && error.stack ? String(error.stack).split("\n").slice(0, 3).join(" | ") : String(error));
});

function describe(error) {
  return error && error.stack ? String(error.stack).split("\n").slice(0, 3).join(" | ") : String(error);
}

async function check(name) {
  pageErrors = [];
  installDom();
  const file = path.join(__dirname, "..", "src", "pages", name + ".js");
  let failure = null;
  try {
    await import(pathToFileURL(file).href + "?check=" + Date.now());
    // The page keeps working after the import resolves, so give the promises it
    // queued a moment to settle before judging it.
    await new Promise((resolve) => setTimeout(resolve, 200));
  } catch (error) {
    failure = describe(error);
  }
  if (!failure && pageErrors.length) failure = pageErrors[0];
  if (failure) {
    console.log("FAIL " + name + ": " + failure);
    return false;
  }
  console.log("ok   " + name);
  return true;
}

(async () => {
  const only = process.argv[2];
  const wanted = only ? [only] : PAGES;
  let failures = 0;
  for (const name of wanted) {
    if (!(await check(name))) failures += 1;
  }
  console.log(failures ? failures + " page(s) failed" : "all pages build");
  // The chrome page is not built the same way, but it has one silent failure mode of
  // its own - being defined and never called - so it is checked here too.
  await import("./check-chrome.js");
  process.exit(failures ? 1 : 0);
})();