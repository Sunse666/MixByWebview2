// The page is a flat list of h2 + card pairs; this folds them into the sections
// the sidebar switches between, and puts the section list in the nav.
import { t } from "../../i18n.js";

const { invoke } = window.__TAURI__.core;

const PREFERRED = [
  "Startup",
  "Performance",
  "Appearance",
  "New tab page",
  "Tab settings",
  "Super drag",
  "Mouse gestures",
  "Shortcuts",
  "Backup",
  "Default browser",
  "Clear browsing data",
];

export function addNavItem(nav, glyph, label, active, page) {
  const item = document.createElement("div");
  item.className = "nav-item" + (active ? " active" : "");
  const icon = document.createElement("span");
  icon.textContent = glyph;
  const text = document.createElement("span");
  text.textContent = t(label);
  item.append(icon, text);
  if (page) {
    item.addEventListener("click", () => {
      invoke("tab_navigate", { url: page }).catch(() => invoke("tab_new", { url: page }).catch(() => {}));
    });
  }
  nav.append(item);
}

export function groupSections(nav, main) {
  const groups = [];
  let current = null;

  for (const child of [...main.children]) {
    if (child.tagName === "H2") {
      current = { title: child.textContent, nodes: [] };
      groups.push(current);
      main.removeChild(child);
      continue;
    }
    if (!current) continue;
    current.nodes.push(child);
    main.removeChild(child);
  }

  const order = groups.filter((group) => group.nodes.length);
  const panels = [];

  for (const group of order) {
    const panel = document.createElement("section");
    panel.className = "section";
    panel.dataset.key = group.title;
    const head = document.createElement("h2");
    head.textContent = group.title;
    panel.append(head, ...group.nodes);
    main.append(panel);
    panels.push(panel);
  }

  const show = (key) => {
    for (const panel of panels) panel.hidden = panel.dataset.key !== key;
    for (const link of nav.querySelectorAll(".nav-section")) {
      link.classList.toggle("active", link.dataset.key === key);
    }
    main.scrollTop = 0;
  };

  const divider = document.createElement("div");
  divider.className = "nav-divider";
  nav.append(divider);

  const rank = (key) => {
    const index = PREFERRED.findIndex((name) => name === key || t(name) === key);
    return index < 0 ? 99 : index;
  };
  panels.sort((a, b) => rank(a.dataset.key) - rank(b.dataset.key));
  main.append(...panels);

  for (const panel of panels) {
    const link = document.createElement("div");
    link.className = "nav-item nav-section";
    link.dataset.key = panel.dataset.key;
    const glyph = document.createElement("span");
    glyph.textContent = "\u00b7";
    const text = document.createElement("span");
    text.textContent = panel.dataset.key;
    link.append(glyph, text);
    link.addEventListener("click", () => show(panel.dataset.key));
    nav.append(link);
  }

  const wanted = decodeURIComponent((location.hash || "").replace(/^#/, ""));
  const match = panels.find((panel) => panel.dataset.key === wanted);
  if (panels.length) show(match ? match.dataset.key : panels[0].dataset.key);
}
