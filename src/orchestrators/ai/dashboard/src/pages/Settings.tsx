import { useState } from "react";
import type { DashboardStatus } from "../types";

interface SettingsProps {
  status: DashboardStatus;
}

export function Settings({ status }: SettingsProps) {
  const [saving, setSaving] = useState(false);
  const [saveResult, setSaveResult] = useState<string | null>(null);
  const config = status.config;

  const handleSave = async (updated: typeof config) => {
    setSaving(true);
    setSaveResult(null);
    try {
      const res = await fetch("/api/settings", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(updated),
      });
      if (!res.ok) {
        throw new Error(`HTTP ${res.status}`);
      }
      setSaveResult("Settings saved.");
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : "Save failed";
      setSaveResult(`Error: ${msg}`);
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="space-y-6">
      <div>
        <h2 className="text-lg font-medium text-gray-100">Settings</h2>
        <p className="text-[12px] text-gray-500">
          Orchestrator configuration
        </p>
      </div>

      {/* Feature Toggles */}
      <section className="bg-[#1a1b23] border border-[#2e303a] rounded-lg p-4">
        <h3 className="text-sm font-medium text-gray-200 mb-3">Features</h3>
        <div className="space-y-3 text-[13px]">
          <div className="flex items-center justify-between">
            <div>
              <span className="text-gray-300">Auto-pull mode</span>
              <p className="text-[11px] text-gray-500">
                Sync models across stones automatically
              </p>
            </div>
            <span className="text-gray-400 font-mono">
              {config.features.auto_pull_mode}
            </span>
          </div>
          <div className="flex items-center justify-between">
            <div>
              <span className="text-gray-300">Delete idle models</span>
              <p className="text-[11px] text-gray-500">
                Remove unused models to free VRAM
              </p>
            </div>
            <span
              className={`font-mono ${
                config.features.delete_on_idle
                  ? "text-emerald-400"
                  : "text-gray-500"
              }`}
            >
              {config.features.delete_on_idle ? "on" : "off"}
            </span>
          </div>
          <div className="flex items-center justify-between">
            <div>
              <span className="text-gray-300">Metrics collection</span>
              <p className="text-[11px] text-gray-500">
                Track request counts and latency
              </p>
            </div>
            <span
              className={`font-mono ${
                config.features.metrics_enabled
                  ? "text-emerald-400"
                  : "text-gray-500"
              }`}
            >
              {config.features.metrics_enabled ? "on" : "off"}
            </span>
          </div>
        </div>
      </section>

      {/* Pinned Models */}
      {Object.keys(config.features.pins).length > 0 && (
        <section className="bg-[#1a1b23] border border-[#2e303a] rounded-lg p-4">
          <h3 className="text-sm font-medium text-gray-200 mb-3">
            Pinned Models
          </h3>
          <div className="space-y-1">
            {Object.entries(config.features.pins).map(([cap, model]) => (
              <div key={cap} className="flex items-center gap-3 text-[12px]">
                <span className="text-gray-400 capitalize w-24">{cap}</span>
                <span className="text-gray-200 font-mono">{model}</span>
              </div>
            ))}
          </div>
        </section>
      )}

      {/* Proxy Configuration */}
      {Object.keys(config.proxies).length > 0 && (
        <section className="bg-[#1a1b23] border border-[#2e303a] rounded-lg p-4">
          <h3 className="text-sm font-medium text-gray-200 mb-3">
            Proxy Ports
          </h3>
          <div className="space-y-1">
            {Object.entries(config.proxies).map(([offering, enabled]) => (
              <div
                key={offering}
                className="flex items-center gap-3 text-[12px]"
              >
                <span className="text-gray-400 capitalize w-32">
                  {offering}
                </span>
                <span
                  className={`font-mono ${
                    enabled ? "text-emerald-400" : "text-gray-600"
                  }`}
                >
                  {enabled ? "ON" : "OFF"}
                </span>
              </div>
            ))}
          </div>
        </section>
      )}

      {/* Raw Config */}
      <section className="bg-[#1a1b23] border border-[#2e303a] rounded-lg p-4">
        <div className="flex items-center justify-between mb-3">
          <h3 className="text-sm font-medium text-gray-200">Raw Config</h3>
          <button
            onClick={() => handleSave(config)}
            disabled={saving}
            className="text-[11px] px-3 py-1 rounded border border-[#2e303a] text-gray-400 hover:text-gray-200 hover:border-gray-500 disabled:opacity-50 transition-colors"
          >
            {saving ? "Saving..." : "Re-apply"}
          </button>
        </div>
        {saveResult && (
          <p
            className={`text-[11px] mb-2 ${
              saveResult.startsWith("Error")
                ? "text-red-400"
                : "text-emerald-400"
            }`}
          >
            {saveResult}
          </p>
        )}
        <pre className="text-[11px] text-gray-400 font-mono overflow-x-auto whitespace-pre-wrap">
          {JSON.stringify(config, null, 2)}
        </pre>
      </section>
    </div>
  );
}
