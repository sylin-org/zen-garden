import { useState, useEffect } from "react";
import { useParams, useNavigate, Link } from "react-router-dom";
import type { DashboardStatus } from "../types";
import { findCatalogEntry } from "../utils/cloudCatalog";

interface CloudEditProps {
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

interface TestResult {
  valid: boolean;
  message: string;
  model_names: string[];
}

export function CloudEdit({ status: _status }: CloudEditProps) {
  const { name } = useParams<{ name: string }>();
  const navigate = useNavigate();

  const catalog = findCatalogEntry(name ?? "");
  const displayName = catalog?.name ?? name ?? "Unknown";

  const [apiKey, setApiKey] = useState("");
  const [priority, setPriority] = useState(-10);
  const [saving, setSaving] = useState(false);
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<TestResult | null>(null);
  const [existing, setExisting] = useState<ConfiguredProvider | null>(null);

  useEffect(() => {
    fetch("/api/providers")
      .then((r) => r.json())
      .then((data: ConfiguredProvider[]) => {
        const found = data.find((p) => p.name === name);
        if (found) {
          setExisting(found);
          setPriority(found.priority);
        }
      })
      .catch(() => {
        // provider list unavailable
      });
  }, [name]);

  async function handleTest() {
    if (!apiKey.trim()) return;
    setTesting(true);
    setTestResult(null);
    try {
      const resp = await fetch("/api/providers/test", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          provider: name,
          api_key: apiKey.trim(),
          base_url: catalog?.baseUrl ?? "",
        }),
      });
      const data = await resp.json();
      setTestResult({
        valid: data.valid,
        message: data.message,
        model_names: data.model_names ?? [],
      });
    } catch (e) {
      setTestResult({
        valid: false,
        message: `Request failed: ${e instanceof Error ? e.message : "unknown"}`,
        model_names: [],
      });
    } finally {
      setTesting(false);
    }
  }

  async function handleSave() {
    if (!apiKey.trim()) return;
    setSaving(true);
    try {
      await fetch("/api/providers", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          kind: name,
          name,
          api_key: apiKey.trim(),
          base_url: catalog?.baseUrl ?? "",
          enabled: true,
          priority,
          capabilities: catalog?.capabilities ?? [],
          models: [],
          cached_models: testResult?.model_names ?? [],
        }),
      });
      navigate(`/infra/cloud/${name}`);
    } finally {
      setSaving(false);
    }
  }

  function handleCancel() {
    navigate(existing ? `/infra/cloud/${name}` : "/infra/cloud");
  }

  return (
    <div className="space-y-5 max-w-3xl">
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
          <Link
            to={`/infra/cloud/${name}`}
            className="text-gray-500 hover:text-gray-300 text-sm"
          >
            {displayName}
          </Link>
          <span className="text-gray-600">/</span>
          <span className="text-gray-100 text-sm font-medium">Edit</span>
        </div>
        <h2 className="text-lg font-medium text-gray-100">
          {existing ? "Edit" : "Configure"} {displayName}
        </h2>
        {catalog && (
          <p className="text-[12px] text-gray-500 mt-1">
            {catalog.description}
          </p>
        )}
      </div>

      {/* Form */}
      <div className="bg-[#1a1b23] border border-[#2e303a] rounded-lg p-4 space-y-4">
        <div>
          <label className="block text-[10px] text-gray-500 mb-1 uppercase tracking-wider">
            API Key
          </label>
          <input
            type="password"
            value={apiKey}
            onChange={(e) => setApiKey(e.target.value)}
            placeholder={`${catalog?.keyPrefix ?? ""}...`}
            className="w-full bg-[#0f1117] border border-[#2e303a] rounded px-3 py-2 text-xs text-gray-200 font-mono focus:border-purple-500 focus:outline-none"
          />
          {existing && (
            <p className="text-[10px] text-gray-600 mt-1">
              Current key: {existing.masked_key} (enter new key to replace)
            </p>
          )}
        </div>

        <div>
          <label className="block text-[10px] text-gray-500 mb-1 uppercase tracking-wider">
            Priority
          </label>
          <input
            type="number"
            value={priority}
            onChange={(e) => setPriority(parseInt(e.target.value) || -10)}
            className="w-32 bg-[#0f1117] border border-[#2e303a] rounded px-3 py-2 text-xs text-gray-200 font-mono focus:border-purple-500 focus:outline-none"
          />
          <p className="text-[10px] text-gray-600 mt-1">
            -10 = cloud fallback (only used when no local instance serves the
            capability). 0 = equal with local. +10 = prefer cloud.
          </p>
        </div>

        {/* Test key */}
        <div className="flex items-center gap-3">
          <button
            onClick={handleTest}
            disabled={testing || !apiKey.trim()}
            className="px-3 py-1.5 text-xs rounded bg-[#2e303a] text-gray-300 hover:bg-[#3e404a] disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {testing ? "Testing..." : "Test Key"}
          </button>
        </div>

        {testResult && (
          <div
            className={`px-3 py-2 rounded text-xs ${
              testResult.valid
                ? "bg-emerald-500/10 border border-emerald-500/30 text-emerald-400"
                : "bg-red-500/10 border border-red-500/30 text-red-400"
            }`}
          >
            <div className="font-mono">
              {testResult.valid ? "Valid" : "Invalid"} &mdash;{" "}
              {testResult.message}
            </div>
            {testResult.valid && testResult.model_names.length > 0 && (
              <div className="mt-1.5 text-[10px] text-gray-400 max-h-32 overflow-y-auto">
                {testResult.model_names.map((mn) => (
                  <span
                    key={mn}
                    className="inline-block mr-1.5 mb-1 px-1.5 py-0.5 rounded bg-[#0f1117] text-gray-300 font-mono"
                  >
                    {mn}
                  </span>
                ))}
              </div>
            )}
          </div>
        )}

        {/* Actions */}
        <div className="flex items-center gap-3 pt-2 border-t border-[#2e303a]">
          <button
            onClick={handleSave}
            disabled={saving || !apiKey.trim()}
            className="px-4 py-1.5 text-xs rounded bg-purple-600 text-white hover:bg-purple-500 disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {saving ? "Saving..." : "Save"}
          </button>
          <button
            onClick={handleCancel}
            className="px-4 py-1.5 text-xs rounded bg-[#2e303a] text-gray-400 hover:bg-[#3e404a]"
          >
            Cancel
          </button>
        </div>
      </div>
    </div>
  );
}
