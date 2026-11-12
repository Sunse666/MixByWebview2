// Browser settings: proxy and default-browser registration.
import { t } from "../../../i18n.js";
import { row, toggle, switchEl } from "../ui.js";

export function buildBrowser({ settings, invoke }) {
  const browserHead = document.createElement("h2");
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

  const proxyHead = document.createElement("h2");
  proxyHead.textContent = t("Network");
  proxyHead.style.margin = "18px 0 10px";

  const proxyCard = document.createElement("div");
  proxyCard.className = "card";
  proxyCard.style.maxWidth = "720px";

  const mode = document.createElement("select");
  mode.className = "button";
  for (const [value, label] of [
    ["system", "Follow the system proxy"],
    ["off", "Connect directly"],
    ["manual", "Manual"],
  ]) {
    const option = document.createElement("option");
    option.value = value;
    option.textContent = t(label);
    if ((settings.proxy_mode || "system") === value) option.selected = true;
    mode.append(option);
  }

  const server = document.createElement("input");
  server.className = "button";
  server.type = "text";
  server.placeholder = "http://127.0.0.1:7890";
  server.value = settings.proxy_server || "";
  server.style.minWidth = "220px";
  const syncServer = () => {
    server.disabled = mode.value !== "manual";
  };
  syncServer();

  mode.addEventListener("change", async () => {
    syncServer();
    await invoke("settings_set", { patch: { proxy_mode: mode.value } }).catch(() => {});
  });
  server.addEventListener("change", async () => {
    await invoke("settings_set", { patch: { proxy_server: server.value.trim() } }).catch(
      () => {},
    );
  });

  proxyCard.append(
    row(t("Proxy"), t("How Mix reaches the network"), mode),
    row(t("Proxy server"), t("Host and port, optionally with a scheme"), server),
    row(t("Restart to apply"), t("The proxy is set when the browser starts")),
  );

  return [browserHead, browserCard, proxyHead, proxyCard];
}
