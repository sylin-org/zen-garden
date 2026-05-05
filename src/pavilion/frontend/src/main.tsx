import { StrictMode } from "react"
import { createRoot } from "react-dom/client"
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow"
import App from "./App"
import { PopoverView } from "./views/Popover"

// IBM Plex (canonical zen-garden typography per tokens.css)
import "@fontsource/ibm-plex-sans/400.css"
import "@fontsource/ibm-plex-sans/500.css"
import "@fontsource/ibm-plex-sans/600.css"
import "@fontsource/ibm-plex-mono/400.css"
import "@fontsource/ibm-plex-mono/500.css"

// Tokens MUST load before component CSS so cascade resolution works.
import "./tokens.css"
import "./App.css"

// Two webviews share one bundle: the main dashboard and the tray
// popover flyout. Branch on the window label so each surface only
// pays for its own React tree (the unrendered branch's modules are
// still loaded, but no DOM/state cost).
const surface = getCurrentWebviewWindow().label === "popover" ? "popover" : "main"

if (surface === "popover") {
  // Popover lives behind a transparent + acrylic chrome; mark the
  // <body> so CSS can drop opaque backgrounds inherited from the
  // main shell tokens.
  document.body.classList.add("popover-body")
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    {surface === "popover" ? <PopoverView /> : <App />}
  </StrictMode>,
)
