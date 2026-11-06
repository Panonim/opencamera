import { i18n } from "@opencamera/i18n/mobile";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import "./index.css";

i18n.setLocale(navigator.languages);

const darkQuery = matchMedia("(prefers-color-scheme: dark)");
const applyTheme = () => document.body.classList.toggle("dark", darkQuery.matches);
applyTheme();
darkQuery.addEventListener("change", applyTheme);

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
