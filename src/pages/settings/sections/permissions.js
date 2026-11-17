// Site permissions: the answers this browser remembers, and undoing them.
import { t } from "../../../i18n.js";

/**
 * One remembered decision per row.
 *
 * The answers used to live in memory only, so a site asked again after every restart.
 * Now that they are written down, they can be looked at - and taken back, one at a time
 * or all at once.
 */
export function buildPermissions({ invoke }) {
  const head = document.createElement("h2");
  head.textContent = t("Site permissions");
  head.style.margin = "18px 0 10px";

  const card = document.createElement("div");
  card.className = "card";
  card.style.maxWidth = "720px";

  const intro = document.createElement("p");
  intro.className = "hint-text";
  intro.textContent = t("Each site asks once; whatever you answer is remembered here.");

  const list = document.createElement("div");
  list.style.display = "flex";
  list.style.flexDirection = "column";
  list.style.gap = "6px";
  list.style.marginTop = "10px";

  const forgetAll = document.createElement("button");
  forgetAll.className = "button";
  forgetAll.textContent = t("Forget every site");
  forgetAll.addEventListener("click", async () => {
    await invoke("permission_forget", {}).catch(() => {});
    await paint();
  });

  const foot = document.createElement("div");
  foot.style.marginTop = "10px";
  foot.append(forgetAll);

  async function paint() {
    const entries = (await invoke("permission_list").catch(() => [])) || [];
    list.replaceChildren();
    forgetAll.disabled = entries.length === 0;
    if (!entries.length) {
      const empty = document.createElement("p");
      empty.className = "hint-text";
      empty.textContent = t("No site has been asked yet.");
      list.append(empty);
      return;
    }
    for (const entry of entries) {
      const row = document.createElement("div");
      row.style.display = "flex";
      row.style.alignItems = "center";
      row.style.gap = "10px";

      const origin = document.createElement("span");
      origin.textContent = entry.origin;
      origin.style.flex = "1";
      origin.style.overflow = "hidden";
      origin.style.textOverflow = "ellipsis";
      origin.style.whiteSpace = "nowrap";

      const kind = document.createElement("span");
      kind.className = "hint-text";
      kind.textContent = entry.kind;

      const verdict = document.createElement("span");
      verdict.className = "hint-text";
      verdict.style.minWidth = "52px";
      verdict.style.textAlign = "right";
      verdict.textContent = t(entry.allow ? "Allowed" : "Blocked");

      const forget = document.createElement("button");
      forget.className = "button";
      forget.textContent = t("Forget");
      forget.addEventListener("click", async () => {
        await invoke("permission_forget", { key: entry.key }).catch(() => {});
        await paint();
      });

      row.append(origin, kind, verdict, forget);
      list.append(row);
    }
  }

  card.append(intro, list, foot);
  paint();
  return [head, card];
}
