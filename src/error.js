import { setLanguage, t } from "./i18n.js";
const { invoke } = window.__TAURI__.core;

const raw = location.hash.replace("#", "");
let target = "";
try {
  target = decodeURIComponent(raw);
} catch (error) {
  target = raw;
}

const looksLikeAddress = /^[a-z]+:\/\//i.test(target) || /^[^\s]+\.[^\s]+$/.test(target);

invoke("settings_get")
  .then((settings) => setLanguage(settings.language || "en"))
  .catch(() => {})
  .finally(paint);

function paint() {
  document.getElementById("headline").textContent = looksLikeAddress
    ? t("This page could not be loaded")
    : t("That does not look like an address");
  document.getElementById("address").textContent = target || t("unknown address");
  document.getElementById("hint").textContent = looksLikeAddress
    ? t("Check the address, your connection, or whether the site is blocking this browser.")
    : t("Type something like example.com, or a full https:// address.");
  document.getElementById("retry").textContent = t("Try again");
  document.getElementById("home").textContent = t("New tab");
  document.title = t("Page not available");
}

document.getElementById("retry").addEventListener("click", () => {
  if (target) invoke("tab_navigate", { url: target }).catch(() => {});
});

document.getElementById("home").addEventListener("click", () => {
  invoke("shortcut", { action: "new_tab" }).catch(() => {});
});
