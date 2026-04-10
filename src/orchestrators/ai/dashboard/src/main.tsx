import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { BrowserRouter } from "react-router-dom";
import { CatalogProvider } from "./contexts/CatalogContext";
import { JobManagerProvider } from "./contexts/JobManagerContext";
import { ActiveRequestProvider } from "./contexts/ActiveRequestManager";
import App from "./App";
import "./index.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <BrowserRouter>
      <CatalogProvider>
        <JobManagerProvider>
          <ActiveRequestProvider>
            <App />
          </ActiveRequestProvider>
        </JobManagerProvider>
      </CatalogProvider>
    </BrowserRouter>
  </StrictMode>,
);
