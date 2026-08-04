import { createApp } from "vue";
import { invoke } from "@tauri-apps/api/core";
import App from "./App.vue";
import "./styles/tokens.css";
import "./styles/app.css";

const startupSplash = document.getElementById("startup-splash");
const startupSplashStartedAt = window.performance.now();
const minimumSplashDurationMs = 850;
let splashDismissed = false;
let splashDismissTimer: number | null = null;

function dismissStartupSplash() {
  if (!startupSplash || splashDismissed) return;
  const remaining = minimumSplashDurationMs - (window.performance.now() - startupSplashStartedAt);
  if (remaining > 0) {
    if (splashDismissTimer === null) {
      splashDismissTimer = window.setTimeout(() => {
        splashDismissTimer = null;
        dismissStartupSplash();
      }, remaining);
    }
    return;
  }
  splashDismissed = true;
  startupSplash.classList.add("startup-splash--leaving");
  window.setTimeout(() => {
    startupSplash.remove();
    void invoke("complete_startup").catch(() => {
      // Browser preview does not expose the native startup windows.
    });
  }, 180);
}

window.addEventListener("nacl-app-ready", dismissStartupSplash, { once: true });
window.setTimeout(dismissStartupSplash, 12_000);

createApp(App).mount("#app");
