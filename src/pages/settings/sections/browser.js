// Generated from the old inline section; behaviour unchanged.
import { t } from "../../../i18n.js";
import { row, toggle, switchEl } from "../ui.js";

export function buildBrowser({ settings, invoke }) {  const browserHead = document.createElement("h2");
  browserHead.textContent = t("Default browser");
  browserHead.style.margin = "18px 0 10px";

  const browserCard = document.createElement("div");
  browserCard.className = "card";
  browserCard.style.maxWidth = "720px";

  const register = document.createElement("button");
  register.className = "button";
  register.textContent = t("Register as a browser");
  const status = document.createElement("span");
  status.className = "card-note";
  register.addEventListener("click", async () => {
    const result = await invoke("register_browser").catch((error) => "error: " + String(error));
    status.textContent = String(result);
  });
  const row2 = document.createElement("div");
  row2.className = "card-actions";
  row2.append(register, status);
  browserCard.append(
    row(t("Default browser"), t("Adds Mix to the Windows default apps list")),
    row2,
  );

  return [browserHead, browserCard];
}
