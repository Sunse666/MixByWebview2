// Bookmarks bar setting.
import { t } from "../../../i18n.js";
import { row, toggle, switchEl } from "../ui.js";

export function buildBookmarks({ settings, invoke }) {
  const row0 = row(
    "Bookmarks bar",
    "Show the bookmarks bar",
    toggle(settings.show_bookmarks, async (value) => {
      await invoke("settings_set", { patch: { show_bookmarks: value } }).catch(() => {});
    }),
  );

  const bookmarksHead = document.createElement("h2");
  bookmarksHead.textContent = t("Bookmarks");
  bookmarksHead.style.margin = "18px 0 10px";
  const bookmarksCard = document.createElement("div");
  bookmarksCard.className = "card";
  bookmarksCard.style.maxWidth = "720px";
  bookmarksCard.append(row0);

  return [bookmarksHead, bookmarksCard];
}
