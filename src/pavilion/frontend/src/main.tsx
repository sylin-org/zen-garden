import { StrictMode } from "react"
import { createRoot } from "react-dom/client"
import App from "./App"

// IBM Plex (canonical zen-garden typography per tokens.css)
import "@fontsource/ibm-plex-sans/400.css"
import "@fontsource/ibm-plex-sans/500.css"
import "@fontsource/ibm-plex-sans/600.css"
import "@fontsource/ibm-plex-mono/400.css"
import "@fontsource/ibm-plex-mono/500.css"

// Tokens MUST load before component CSS so cascade resolution works.
import "./tokens.css"
import "./App.css"

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
)
