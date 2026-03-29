import { useState, useEffect } from "react";
import { useParams, Link } from "react-router-dom";
import type { DashboardStatus, InstanceStatus } from "../types";
import { findCatalogEntry, CAP_COLORS } from "../utils/cloudCatalog";

interface CloudDetailProps {
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

export function CloudDetail({ status }: CloudDetailProps) {
  const { name } = useParams<{ name: string }>();
  const [provider, setProvider] = useState<ConfiguredProvider | null>(null);
  const [loaded, setLoaded] = useState(false);

  const catalog = findCatalogEntry(name ?? "");

  useEffect(() => {
    fetch("/api/providers")
      .then((r) => r.json())
      .then((data: ConfiguredProvider[]) => {
        const found = data.find((p) => p.name === name);
        setProvider(found ?? null);
        setLoaded(true);
      })
      .catch(() => setLoaded(true));
  }, [name]);

  // Find instances that match this cloud provider
  const instances: InstanceStatus[] = status.instances.filter(
    (i) => i.kind === name,
  );

  // Models from this cloud provider
  const models = status.models.filter((m) =>
    m.available_on.some((p) => p.offering === name),
  );

  const displayName = catalog?.name ?? name ?? "Unknown";

  if (loaded && !provider) {
    return (
      <div className="space-y-4 max-w-5xl">
        <div className="flex items-center gap-2 mb-1">
          <Link to="/" className="text-gray-500 hover:text-gray-300 text-sm">
            Overview
          </Link>
          <span className="text-gray-600">/</span>
          <Link
            to="/infra/cloud"
            className="text-gray-500 hover:text-gray-300 text-sm"
          >
            Cloud
          </Link>
          <span className="text-gray-600">/</span>
          <span className="text-gray-100 text-sm font-medium">
            {displayName}
          </span>
        </div>
        <div className="bg-[#1a1b23] border border-[#2e303a] rounded-lg p-6 text-center">
          <p className="text-sm text-gray-400 mb-2">
            {displayName} is not configured yet.
          </p>
          <Link
            to={`/infra/cloud/${name}/edit`}
            className="text-xs px-3 py-1.5 rounded bg-purple-500/20 text-purple-300 hover:bg-purple-500/30 inline-block"
          >
            Add API Key
          </Link>
        </div>
      </div>
    );
  }

  if (!loaded) {
    return (
      <div className="flex items-center justify-center h-64">
        <span className="text-sm text-gray-500">Loading...</span>
      </div>
    );
  }

  const inst = instances[0];

  return (
    <div className="space-y-5 max-w-5xl">
      {/* Breadcrumb */}
      <div>
        <div className="flex items-center gap-2 mb-1">
          <Link to="/" className="text-gray-500 hover:text-gray-300 text-sm">
            Overview
          </Link>
          <span className="text-gray-600">/</span>
          <Link
            to="/infra/cloud"
            className="text-gray-500 hover:text-gray-300 text-sm"
          >
            Cloud
          </Link>
          <span className="text-gray-600">/</span>
          <span className="text-gray-100 text-sm font-medium">
            {displayName}
          </span>
        </div>
        <h2 className="text-lg font-medium text-gray-100">{displayName}</h2>
      </div>

      {/* Provider status card */}
      <div className="bg-[#1a1b23] border border-purple-500/30 rounded-lg p-4">
        <div className="flex items-center justify-between mb-3">
          <div className="flex items-center gap-3">
            <span className="text-[10px] px-1.5 py-0.5 rounded bg-purple-500/20 text-purple-400">
              cloud
            </span>
            {inst && (
              <span className="flex items-center gap-1.5 text-[11px] text-gray-500">
                <span
                  className={`w-1.5 h-1.5 rounded-full ${
                    inst.health === "healthy"
                      ? "bg-emerald-400"
                      : "bg-red-400"
                  }`}
                />
                {inst.health === "healthy" ? "connected" : "unreachable"}
              </span>
            )}
          </div>
          <Link
            to={`/infra/cloud/${name}/edit`}
            className="text-xs px-2 py-1 rounded bg-[#2e303a] text-gray-300 hover:bg-[#3e404a]"
          >
            Edit Configuration
          </Link>
        </div>

        <div className="grid grid-cols-2 gap-3 text-[12px]">
          {provider && (
            <>
              <div>
                <span className="text-gray-500">API Key</span>
                <span className="ml-2 text-gray-300 font-mono">
                  {provider.masked_key}
                </span>
              </div>
              <div>
                <span className="text-gray-500">Priority</span>
                <span className="ml-2 text-gray-300 font-mono">
                  {provider.priority}
                </span>
              </div>
            </>
          )}
        </div>

        {/* Capabilities */}
        <div className="mt-3">
          <span className="text-[10px] text-gray-500 uppercase tracking-wider font-semibold">
            Capabilities
          </span>
          <div className="flex flex-wrap gap-1 mt-1">
            {(catalog?.capabilities ?? provider?.capabilities ?? []).map(
              (cap) => (
                <Link
                  key={cap}
                  to={`/capability/${cap}`}
                  className={`text-[10px] px-1.5 py-0.5 rounded font-mono hover:opacity-80 ${
                    CAP_COLORS[cap] ?? "bg-gray-700 text-gray-400"
                  }`}
                >
                  {cap}
                </Link>
              ),
            )}
          </div>
        </div>
      </div>

      {/* Model list */}
      {models.length > 0 && (
        <div className="bg-[#1a1b23] border border-[#2e303a] rounded-lg overflow-hidden">
          <div className="px-4 py-2 border-b border-[#2e303a]">
            <span className="text-sm font-medium text-gray-200">Models</span>
            <span className="ml-2 text-[11px] text-gray-500">
              {models.length} available
            </span>
          </div>
          <div className="divide-y divide-[#2e303a]/30">
            {models.map((model) => (
              <div
                key={model.name}
                className="px-4 py-1.5 flex items-center justify-between text-[12px] hover:bg-[#22232d]"
              >
                <span className="font-mono text-gray-300">{model.name}</span>
                <div className="flex items-center gap-2">
                  {model.capabilities.map((c) => (
                    <span
                      key={c}
                      className={`text-[10px] px-1 py-0.5 rounded font-mono ${
                        CAP_COLORS[c] ?? "bg-gray-700 text-gray-400"
                      }`}
                    >
                      {c}
                    </span>
                  ))}
                </div>
              </div>
            ))}
          </div>
        </div>
      )}

      {models.length === 0 && (
        <div className="bg-[#1a1b23] border border-[#2e303a] rounded-lg p-4 text-center">
          <p className="text-[12px] text-gray-500">
            No cached models. Models will appear after the provider is queried.
          </p>
        </div>
      )}

      <div>
        <Link
          to="/infra/cloud"
          className="text-[12px] text-gray-500 hover:text-gray-300"
        >
          &larr; Back to cloud providers
        </Link>
      </div>
    </div>
  );
}
