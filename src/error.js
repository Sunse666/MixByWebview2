const { invoke } = window.__TAURI__.core;

const raw = location.hash.replace("#", "");
let target = "";
try {
  target = decodeURIComponent(raw);
} catch (error) {
  target = raw;
}

const looksLikeAddress = /^[a-z]+:\/\//i.test(target) || /^[^\s]+\.[^\s]+$/.test(target);

document.getElementById("headline").textContent = looksLikeAddress
  ? "This page could not be loaded"
  : "That does not look like an address";
document.getElementById("address").textContent = target || "unknown address";
document.getElementById("hint").textContent = looksLikeAddress
  ? "Check the address, your connection, or whether the site is blocking this browser."
  : "Type something like example.com, or a full https:// address.";

document.getElementById("retry").addEventListener("click", () => {
  if (target) invoke("tab_navigate", { url: target }).catch(() => {});
});

document.getElementById("home").addEventListener("click", () => {
  invoke("shortcut", { action: "new_tab" }).catch(() => {});
});
