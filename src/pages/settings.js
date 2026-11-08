import { setLanguage, t } from "../i18n.js";
import { withFeedback, setFeedbackText } from "./feedback.js";
import { addNavItem, groupSections } from "./settings/shell.js";
import { buildClearData } from "./settings/sections/clear-data.js";
import { buildStartup } from "./settings/sections/startup.js";
import { buildBookmarks } from "./settings/sections/bookmarks.js";
import { buildSearch } from "./settings/sections/search.js";
import { buildLanguage } from "./settings/sections/language.js";
import { buildAppearance } from "./settings/sections/appearance.js";
import { buildPerformance } from "./settings/sections/performance.js";
import { buildTabs } from "./settings/sections/tabs.js";
import { buildDrag } from "./settings/sections/drag.js";
import { buildGestures } from "./settings/sections/gestures.js";
import { buildNewtab } from "./settings/sections/newtab.js";
import { buildShortcuts } from "./settings/sections/shortcuts.js";
import { buildBackup } from "./settings/sections/backup.js";
import { buildBrowser } from "./settings/sections/browser.js";
import { buildToolbar } from "./settings/sections/toolbar.js";

// Every section saves as you change it; this wrapper says whether it worked.
const invoke = withFeedback(window.__TAURI__.core.invoke);

const nav = document.getElementById("nav");
const main = document.getElementById("main");

// The page is nothing but an order: each section module returns its own heading
// and card, and groupSections folds them into the sidebar afterwards. The order
// here is only the reading order before folding; the sidebar sorts by PREFERRED
// in shell.js.
const SECTIONS = [
  buildStartup,
  buildBookmarks,
  buildSearch,
  buildLanguage,
  buildAppearance,
  buildPerformance,
  buildTabs,
  buildDrag,
  buildGestures,
  buildNewtab,
  buildShortcuts,
  buildBackup,
  buildBrowser,
  buildToolbar,
  buildClearData,
];

async function boot() {
  const settings = await invoke("settings_get").catch(() => null);
  setLanguage((settings && settings.language) || "en");
  setFeedbackText(t("Saved"), t("Could not save:"));
  document.title = t("Settings");

  const pages = {
    history: "http://tauri.localhost/pages/history.html",
    extensions: "http://tauri.localhost/pages/extensions.html",
  };
  addNavItem(nav, "⟲", "History", false, pages.history);
  addNavItem(nav, "☆", "Bookmarks", false, null);
  addNavItem(nav, "⧉", "Extensions", false, pages.extensions);
  addNavItem(nav, "⚙", "Settings", true, null);

  const ctx = { settings, invoke };
  for (const build of SECTIONS) {
    main.append(...build(ctx));
  }
}

boot().then(() => {
  groupSections(nav, main);
});
