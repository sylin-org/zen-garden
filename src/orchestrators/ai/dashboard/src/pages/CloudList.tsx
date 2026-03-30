import { useState, useEffect, useCallback } from "react";
import { Link } from "react-router-dom";
import type { DashboardStatus, ConfiguredProvider } from "../types";
import { CLOUD_CATALOG, CAP_COLORS } from "../utils/cloudCatalog";

interface CloudListProps {
  status: DashboardStatus;
}

export function CloudList({ status }: CloudListProps) {
  const [providers, setProviders] = useState<ConfiguredProvider[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [deleting, setDeleting] = useState<string | null>(null);

  const reload = useCallback(() => {
    fetch("/api/providers")
      .then((r) => r.json())
      .then((data: ConfiguredProvider[]) => {
        setProviders(data);
        setLoaded(true);
      })
      .catch(() => setLoaded(true));
  }, []);

  useEffect(() => {
    if (!loaded) reload();
  }, [loaded, reload]);

  async function handleDelete(providerName: string) {
    if (!confirm(`Remove cloud provider key "${providerName}"?`)) return;
    setDeleting(providerName);
    try {
      await fetch(`/api/providers/${providerName}`, { method: "DELETE" });
      reload();
    } finally {
      setDeleting(null);
    }
  }

  const [toggling, setToggling] = useState<string | null>(null);

  async function handleToggle(providerName: string) {
    setToggling(providerName);
    try {
      await fetch(`/api/providers/${providerName}/toggle`, { method: "PATCH" });
      reload();
    } finally {
      setToggling(null);
    }
  }

  // Group configured providers by kind
  const byKind = new Map<string, ConfiguredProvider[]>();
  for (const p of providers) {
    const list = byKind.get(p.kind) ?? [];
    list.push(p);
    byKind.set(p.kind, list);
  }

  // Summary stats
  const totalKeys = providers.length;
  const healthyKeys = status.instances.filter(
    (i) => i.stone_name.startsWith("cloud:") && i.health === "healthy",
  ).length;
  const unhealthyKeys = status.instances.filter(
    (i) =>
      i.stone_name.startsWith("cloud:") &&
      i.health !== "healthy" &&
      i.health !== "profiling",
  ).length;

  return (
    <div className="space-y-6 max-w-5xl">
      {/* Header */}
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
          Manage API keys for cloud AI providers. Each provider kind supports
          multiple keys (e.g., work &amp; personal accounts).
        </p>
      </div>

      {/* Summary panel */}
      {totalKeys > 0 && (
        <div className="flex gap-4">
          <div className="px-4 py-3 rounded-lg bg-[#1a1b23] border border-[#2e303a]">
            <div className="text-2xl font-semibold text-gray-100">
              {totalKeys}
            </div>
            <div className="text-[10px] text-gray-500 uppercase tracking-wider">
              {totalKeys === 1 ? "Key" : "Keys"} configured
            </div>
          </div>
          {healthyKeys > 0 && (
            <div className="px-4 py-3 rounded-lg bg-emerald-500/5 border border-emerald-500/20">
              <div className="text-2xl font-semibold text-emerald-400">
                {healthyKeys}
              </div>
              <div className="text-[10px] text-emerald-400/70 uppercase tracking-wider">
                Healthy
              </div>
            </div>
          )}
          {unhealthyKeys > 0 && (
            <div className="px-4 py-3 rounded-lg bg-red-500/5 border border-red-500/20">
              <div className="text-2xl font-semibold text-red-400">
                {unhealthyKeys}
              </div>
              <div className="text-[10px] text-red-400/70 uppercase tracking-wider">
                Degraded
              </div>
            </div>
          )}
        </div>
      )}

      {/* Provider kinds — one section each */}
      {CLOUD_CATALOG.map((catalog) => {
        const keys = byKind.get(catalog.id) ?? [];
        const hasKeys = keys.length > 0;

        return (
          <div
            key={catalog.id}
            className={`rounded-lg border p-4 ${
              hasKeys
                ? "border-purple-500/40 bg-[#1a1b23]"
                : "border-[#2e303a] bg-[#1a1b23]/50"
            }`}
          >
            {/* Provider kind header */}
            <div className="flex items-start justify-between mb-3">
              <div>
                <div className="flex items-center gap-2">
                  <h3 className="text-sm font-semibold text-gray-100">
                    {catalog.name}
                  </h3>
                  {hasKeys ? (
                    <span className="text-[10px] px-1.5 py-0.5 rounded bg-purple-500/20 text-purple-300 font-mono">
                      {keys.length} {keys.length === 1 ? "key" : "keys"}
                    </span>
                  ) : (
                    <span className="text-[10px] px-1.5 py-0.5 rounded bg-gray-700 text-gray-400">
                      not configured
                    </span>
                  )}
                </div>
                <p className="text-[11px] text-gray-500 mt-0.5">
                  {catalog.description}
                </p>
                <div className="flex flex-wrap gap-1 mt-1.5">
                  {catalog.capabilities.map((cap) => (
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

              <Link
                to={`/infra/cloud/${catalog.id}/edit?new=true`}
                className="text-xs px-3 py-1.5 rounded bg-purple-500/20 text-purple-300 hover:bg-purple-500/30 whitespace-nowrap"
              >
                + Add Key
              </Link>
            </div>

            {/* Key table */}
            {hasKeys && (
              <div className="border-t border-[#2e303a] pt-2 mt-2">
                <table className="w-full text-xs">
                  <thead>
                    <tr className="text-[10px] text-gray-500 uppercase tracking-wider">
                      <th className="text-left py-1 font-medium">Name</th>
                      <th className="text-left py-1 font-medium">Key</th>
                      <th className="text-left py-1 font-medium">Priority</th>
                      <th className="text-left py-1 font-medium">Status</th>
                      <th className="text-center py-1 font-medium">Enabled</th>
                      <th className="text-right py-1 font-medium">Actions</th>
                    </tr>
                  </thead>
                  <tbody>
                    {keys.map((key) => {
                      const inst = status.instances.find(
                        (i) =>
                          i.stone_name === `cloud:${key.name}` &&
                          i.kind === key.kind,
                      );
                      const isHealthy = inst?.health === "healthy";
                      const isProfiling = inst?.health === "profiling";
                      const modelCount = inst?.models_available.length ?? 0;

                      return (
                        <tr
                          key={key.name}
                          className={`border-t border-[#2e303a]/50 ${
                            !key.enabled ? "opacity-40" : ""
                          }`}
                        >
                          <td className="py-2 font-mono text-gray-200">
                            <Link
                              to={`/infra/cloud/${key.name}`}
                              className="hover:text-purple-300"
                            >
                              {key.name}
                            </Link>
                          </td>
                          <td className="py-2 font-mono text-gray-500">
                            {key.masked_key}
                          </td>
                          <td className="py-2 text-gray-400">{key.priority}</td>
                          <td className="py-2">
                            {isProfiling ? (
                              <span className="text-yellow-400">
                                probing...
                              </span>
                            ) : isHealthy ? (
                              <span className="text-emerald-400">
                                healthy
                                {modelCount > 0 && (
                                  <span className="text-gray-500 ml-1">
                                    ({modelCount} models)
                                  </span>
                                )}
                              </span>
                            ) : (
                              <span className="text-red-400">unhealthy</span>
                            )}
                          </td>
                          <td className="py-2 text-center">
                            <button
                              onClick={() => handleToggle(key.name)}
                              disabled={toggling === key.name}
                              className={`relative inline-flex h-5 w-9 items-center rounded-full transition-colors focus:outline-none disabled:opacity-50 ${
                                key.enabled
                                  ? "bg-purple-600"
                                  : "bg-gray-600"
                              }`}
                              title={key.enabled ? "Disable this provider" : "Enable this provider"}
                            >
                              <span
                                className={`inline-block h-3.5 w-3.5 transform rounded-full bg-white transition-transform ${
                                  key.enabled
                                    ? "translate-x-4.5"
                                    : "translate-x-0.5"
                                }`}
                              />
                            </button>
                          </td>
                          <td className="py-2 text-right">
                            <div className="flex items-center justify-end gap-2">
                              <Link
                                to={`/infra/cloud/${key.name}/edit`}
                                className="px-2 py-0.5 rounded bg-[#2e303a] text-gray-400 hover:bg-[#3e404a] hover:text-gray-200"
                              >
                                Edit
                              </Link>
                              <button
                                onClick={() => handleDelete(key.name)}
                                disabled={deleting === key.name}
                                className="px-2 py-0.5 rounded bg-red-500/10 text-red-400 hover:bg-red-500/20 disabled:opacity-50"
                              >
                                {deleting === key.name
                                  ? "..."
                                  : "Remove"}
                              </button>
                            </div>
                          </td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}
