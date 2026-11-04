import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "./styles/globals.css";
import { WidgetView } from "./components/widget-view";

document.body.style.background = "transparent";
document.documentElement.style.background = "transparent";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <WidgetView />
  </StrictMode>,
);
