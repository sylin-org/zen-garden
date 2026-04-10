import {
  createContext,
  useContext,
  useEffect,
  useState,
  useCallback,
  type ReactNode,
} from "react";
import { get } from "../api/client";
import type { CatalogSummary } from "../api/types";
import { useSSE } from "../hooks/useSSE";

interface CatalogState {
  catalog: CatalogSummary | null;
  loading: boolean;
  error: string | null;
  refresh: () => void;
}

const CatalogContext = createContext<CatalogState>({
  catalog: null,
  loading: true,
  error: null,
  refresh: () => {},
});

export function CatalogProvider({ children }: { children: ReactNode }) {
  const [catalog, setCatalog] = useState<CatalogSummary | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const fetchCatalog = useCallback(async () => {
    try {
      const data = await get<CatalogSummary>("/v1/catalog");
      setCatalog(data);
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to load catalog");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchCatalog();
  }, [fetchCatalog]);

  // Re-fetch when catalog version changes
  useSSE({
    focus: "catalog.version",
    onEvent: () => {
      fetchCatalog();
    },
  });

  return (
    <CatalogContext.Provider value={{ catalog, loading, error, refresh: fetchCatalog }}>
      {children}
    </CatalogContext.Provider>
  );
}

export function useCatalog(): CatalogState {
  return useContext(CatalogContext);
}
