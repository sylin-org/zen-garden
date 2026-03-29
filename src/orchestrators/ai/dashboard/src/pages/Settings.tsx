import { useState } from "react";
import type { DashboardStatus, InferenceDefaults } from "../types";
import { ALL_CAPABILITIES, CAPABILITY_LABELS } from "../types";

interface SettingsProps {
  status: DashboardStatus;
}

export function Settings({ status }: SettingsProps) {
  const [saving, setSaving] = useState(false);
  const [saveResult, setSaveResult] = useState<string | null>(null);
  const [defaultsSaving, setDefaultsSaving] = useState(false);
  const [defaultsResult, setDefaultsResult] = useState<string | null>(null);
  const config = status.config;

  // Local state for editable defaults.
  const [defaults, setDefaults] = useState<Record<string, InferenceDefaults>>(
    () => config.defaults ?? {}
  );

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

  const handleSaveDefaults = async () => {
    setDefaultsSaving(true);
    setDefaultsResult(null);
    try {
      // Strip empty entries (all fields null/undefined).
      const cleaned: Record<string, InferenceDefaults> = {};
      for (const [cap, d] of Object.entries(defaults)) {
        if (d.temperature != null || d.max_tokens != null || d.top_p != null) {
          cleaned[cap] = d;
        }
      }
      const res = await fetch("/api/defaults", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(cleaned),
      });
      if (!res.ok) {
        throw new Error(`HTTP ${res.status}`);
      }
      setDefaults(cleaned);
      setDefaultsResult("Defaults saved.");
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : "Save failed";
      setDefaultsResult(`Error: ${msg}`);
    } finally {
      setDefaultsSaving(false);
    }
  };

  const updateDefault = (
    capability: string,
    field: keyof InferenceDefaults,
    value: string
  ) => {
    setDefaults((prev) => {
      const entry = prev[capability] ?? {};
      const parsed = value.trim() === "" ? null : Number(value);
      const numValue =
        parsed !== null && !isNaN(parsed) ? parsed : null;
      const updated = { ...entry, [field]: numValue };
      return { ...prev, [capability]: updated };
    });
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

      {/* Inference Defaults */}
      <section className="bg-[#1a1b23] border border-[#2e303a] rounded-lg p-4">
        <div className="flex items-center justify-between mb-3">
          <div>
            <h3 className="text-sm font-medium text-gray-200">
              Inference Defaults
            </h3>
            <p className="text-[11px] text-gray-500">
              Default parameters injected when clients don't specify them
            </p>
          </div>
          <button
            onClick={handleSaveDefaults}
            disabled={defaultsSaving}
            className="text-[11px] px-3 py-1 rounded border border-[#2e303a] text-gray-400 hover:text-gray-200 hover:border-gray-500 disabled:opacity-50 transition-colors"
          >
            {defaultsSaving ? "Saving..." : "Save defaults"}
          </button>
        </div>
        {defaultsResult && (
          <p
            className={`text-[11px] mb-2 ${
              defaultsResult.startsWith("Error")
                ? "text-red-400"
                : "text-emerald-400"
            }`}
          >
            {defaultsResult}
          </p>
        )}
        <div className="overflow-x-auto">
          <table className="w-full text-[12px]">
            <thead>
              <tr className="text-gray-500 border-b border-[#2e303a]">
                <th className="text-left py-2 pr-4 font-medium">Capability</th>
                <th className="text-left py-2 px-2 font-medium">Temperature</th>
                <th className="text-left py-2 px-2 font-medium">Max tokens</th>
                <th className="text-left py-2 px-2 font-medium">Top-p</th>
              </tr>
            </thead>
            <tbody>
              {ALL_CAPABILITIES.map((cap) => {
                const d = defaults[cap] ?? {};
                return (
                  <tr
                    key={cap}
                    className="border-b border-[#2e303a]/50 hover:bg-[#22232d]"
                  >
                    <td className="py-1.5 pr-4 text-gray-300 capitalize">
                      {CAPABILITY_LABELS[cap] ?? cap}
                    </td>
                    <td className="py-1.5 px-2">
                      <input
                        type="number"
                        step="0.1"
                        min="0"
                        max="2"
                        placeholder="--"
                        value={d.temperature ?? ""}
                        onChange={(e) =>
                          updateDefault(cap, "temperature", e.target.value)
                        }
                        className="w-20 bg-[#12131a] border border-[#2e303a] rounded px-2 py-0.5 text-gray-300 font-mono text-[11px] placeholder:text-gray-600 focus:border-gray-500 focus:outline-none"
                      />
                    </td>
                    <td className="py-1.5 px-2">
                      <input
                        type="number"
                        step="256"
                        min="1"
                        placeholder="--"
                        value={d.max_tokens ?? ""}
                        onChange={(e) =>
                          updateDefault(cap, "max_tokens", e.target.value)
                        }
                        className="w-24 bg-[#12131a] border border-[#2e303a] rounded px-2 py-0.5 text-gray-300 font-mono text-[11px] placeholder:text-gray-600 focus:border-gray-500 focus:outline-none"
                      />
                    </td>
                    <td className="py-1.5 px-2">
                      <input
                        type="number"
                        step="0.05"
                        min="0"
                        max="1"
                        placeholder="--"
                        value={d.top_p ?? ""}
                        onChange={(e) =>
                          updateDefault(cap, "top_p", e.target.value)
                        }
                        className="w-20 bg-[#12131a] border border-[#2e303a] rounded px-2 py-0.5 text-gray-300 font-mono text-[11px] placeholder:text-gray-600 focus:border-gray-500 focus:outline-none"
                      />
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
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
