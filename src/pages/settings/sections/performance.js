// Generated from the old inline section; behaviour unchanged.
import { t } from "../../../i18n.js";
import { row, toggle, switchEl } from "../ui.js";

export function buildPerformance({ settings, invoke }) {  const perfHead = document.createElement("h2");
  perfHead.textContent = t("Performance");
  perfHead.style.margin = "18px 0 10px";

  const perfCard = document.createElement("div");
  perfCard.className = "card";
  perfCard.style.maxWidth = "720px";

  const perfSwitch = switchEl(settings.memory_saver !== false, async (next) => {
    await invoke("settings_set", { patch: { memory_saver: next } }).catch(() => {});
    liveInput.disabled = !next;
  });

  const liveInput = document.createElement("input");
  liveInput.type = "range";
  liveInput.min = "2";
  liveInput.max = "40";
  liveInput.value = String(settings.max_live_tabs ?? 12);
  liveInput.style.width = "220px";
  liveInput.disabled = settings.memory_saver === false;
  const liveValue = document.createElement("span");
  liveValue.className = "card-note";
  liveValue.textContent = liveInput.value;
  liveInput.addEventListener("input", () => {
    liveValue.textContent = liveInput.value;
  });
  liveInput.addEventListener("change", async () => {
    await invoke("settings_set", {
      patch: { max_live_tabs: Number(liveInput.value) },
    }).catch(() => {});
  });
  const liveHolder = document.createElement("div");
  liveHolder.style.display = "flex";
  liveHolder.style.gap = "10px";
  liveHolder.style.alignItems = "center";
  liveHolder.append(liveInput, liveValue);

  const keepAlive = document.createElement("input");
  keepAlive.className = "hex";
  keepAlive.style.width = "320px";
  keepAlive.placeholder = "bilibili.com, mail.google.com";
  keepAlive.value = settings.keep_alive_hosts || "";
  keepAlive.addEventListener("change", async () => {
    await invoke("settings_set", { patch: { keep_alive_hosts: keepAlive.value } }).catch(() => {});
  });

  const markSwitch = switchEl(settings.mark_suspended !== false, async (next) => {
    await invoke("settings_set", { patch: { mark_suspended: next } }).catch(() => {});
  });

  perfCard.append(
    row(
      t("Memory saver"),
      t("Tabs beyond the limit are suspended and reload when selected"),
      perfSwitch,
    ),
    row(t("Keep this many tabs live"), "", liveHolder),
    row(t("Never suspend these sites"), t("Comma separated hosts"), keepAlive),
    row(t("Mark suspended tabs"), t("Dashed border and grey icon"), markSwitch),
  );

  return [perfHead, perfCard];
}
