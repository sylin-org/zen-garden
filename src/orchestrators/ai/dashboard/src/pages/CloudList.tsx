import { useState, useEffect } from "react";
import { Link } from "react-router-dom";
import type { DashboardStatus } from "../types";
import { CLOUD_CATALOG, CAP_COLORS } from "../utils/cloudCatalog";

interface CloudListProps {
  status: DashboardStatus;
}

interface ConfiguredProvider {
  name: string;
  kind: string;
  base_url: string;
  masked_key: string;
  enabled: boolean;
  priority: number;
  capabilities: string[];
  model_count: number;
}

export function CloudList({ status: _status }: CloudListProps) {
  const [providers, setProviders] = useState<ConfiguredProvider[]>([]);
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    if (!loaded) {
      fetch("/api/providers")
        .then((r) => r.json())
        .then((data: ConfiguredProvider[]) => {
          setProviders(data);
          setLoaded(true);
        })
        .catch(() => setLoaded(true));
    }
  }, [loaded]);

  const getConfigured = (id: string) => providers.find((p) => p.name === id);

  return (
    <div className="space-y-6 max-w-5xl">
      <div>
        <div className="flex items-center gap-2 mb-1">
          <Link to="/" className="text-gray-500 hover:text-gray-300 text-sm">
            Overview
          </Link>
          <span className="text-gray-600">/</span>
          <span className="text-gray-100 text-sm font-medium">
            Cloud Providers
          </span>
        </div>
        <h2 className="text-lg font-medium text-gray-100">Cloud Providers</h2>
        <p className="text-[12px] text-gray-500">
          Add cloud AI providers as fallback or supplementary capability
          sources.
        </p>
      </div>

      <div className="space-y-3">
        {CLOUD_CATALOG.map((provider) => {
          const configured = getConfigured(provider.id);

          return (
            <div
              key={provider.id}
              className={`rounded-lg border p-4 ${
                configured
                  ? "border-purple-500/40 bg-[#1a1b23]"
                  : "border-[#2e303a] bg-[#1a1b23]/50"
              }`}
            >
              <div className="flex items-start justify-between">
                <div className="flex-1">
                  <div className="flex items-center gap-3 mb-1">
                    <h3 className="text-sm font-semibold text-gray-100">
                      {provider.name}
                    </h3>
                    {configured ? (
                      <span className="text-[10px] px-1.5 py-0.5 rounded bg-purple-500/20 text-purple-300 font-mono">
                        configured
                      </span>
                    ) : (
                      <span className="text-[10px] px-1.5 py-0.5 rounded bg-gray-700 text-gray-400">
                        not configured
                      </span>
                    )}
                    {configured && (
                      <span className="text-[10px] text-gray-500 font-mono">
                        key: {configured.masked_key} &middot; priority:{" "}
                        {configured.priority}
                        {configured.model_count > 0 && (
                          <>
                            {" "}
                            &middot; {configured.model_count} model
                            {configured.model_count !== 1 ? "s" : ""}
                          </>
                        )}
                      </span>
                    )}
                  </div>

                  <p className="text-xs text-gray-500 mb-2">
                    {provider.description}
                  </p>

                  <div className="flex flex-wrap gap-1">
                    {provider.capabilities.map((cap) => (
                      <span
                        key={cap}
                        className={`text-[10px] px-1.5 py-0.5 rounded font-mono ${
                          CAP_COLORS[cap] ?? "bg-gray-700 text-gray-400"
                        }`}
                      >
                        {cap}
                      </span>
                    ))}
                  </div>
                </div>

                <div className="ml-4">
                  {configured ? (
                    <Link
                      to={`/infra/cloud/${provider.id}`}
                      className="text-xs px-3 py-1 rounded bg-purple-500/20 text-purple-300 hover:bg-purple-500/30 inline-block"
                    >
                      View
                    </Link>
                  ) : (
                    <Link
                      to={`/infra/cloud/${provider.id}/edit`}
                      className="text-xs px-3 py-1 rounded bg-purple-500/20 text-purple-300 hover:bg-purple-500/30 inline-block"
                    >
                      Add API Key
                    </Link>
                  )}
                </div>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
