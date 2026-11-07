// Generated from the old inline section; behaviour unchanged.
import { t } from "../../../i18n.js";
import { row, toggle, switchEl } from "../ui.js";

export function buildStartup({ settings, invoke }) {  const restoreRow = row(
    "Restore session",
    "Restore the last session on start",
    toggle(settings.restore_session, async (value) => {
      await invoke("settings_set", { patch: { restore_session: value } }).catch(() => {});
    }),
  );

  const startHead = document.createElement("h2");
  startHead.textContent = t("Startup");
  startHead.style.margin = "18px 0 10px";
  const startCard = document.createElement("div");
  startCard.className = "card";
  startCard.style.maxWidth = "720px";

  const startUrl = document.createElement("input");
  startUrl.className = "hex";
  startUrl.style.width = "320px";
  startUrl.value = settings.start_url || "";
  startUrl.addEventListener("change", async () => {
    await invoke("settings_set", { patch: { start_url: startUrl.value } }).catch(() => {});
  });
  const homeUrl = document.createElement("input");
  homeUrl.className = "hex";
  homeUrl.style.width = "320px";
  homeUrl.placeholder = settings.start_url || "https://example.com";
  homeUrl.value = settings.home_url || "";
  homeUrl.addEventListener("change", async () => {
    await invoke("settings_set", { patch: { home_url: homeUrl.value } }).catch(() => {});
  });
  startCard.append(
    restoreRow,
    row("Start page", "Opened at launch", startUrl),
    row(t("Home address"), t("Opened by the home button; empty uses the start page"), homeUrl),
  );

  return [startHead, startCard];
}
