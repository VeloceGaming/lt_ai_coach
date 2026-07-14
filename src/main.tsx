import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./ui/App";
import { initTheme } from "./ui/stores/useThemeStore";
import { initI18n } from "./ui/stores/useI18nStore";
import "./ui/tokens.css";
import "./ui/styles.css";

// Apply the saved theme before the first paint so there's no flash of default.
initTheme();
// Load the saved language choice (if any translations are on disk).
initI18n();

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
