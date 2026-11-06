import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import "./index.css";
import { startPeriodicUpdateCheck } from "./lib/updates";
import { initStore } from "./store";

void initStore();
startPeriodicUpdateCheck();

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
