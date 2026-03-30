import { useState, useEffect } from "react";
import { useParams, useNavigate, useSearchParams, Link } from "react-router-dom";
import type { DashboardStatus, ConfiguredProvider } from "../types";
import { findCatalogEntry } from "../utils/cloudCatalog";

interface CloudEditProps {
  status: DashboardStatus;
}

interface TestResult {
  valid: boolean;
  message: string;
  model_names: string[];
}

function isValidLocatorName(value: string): boolean {
  return /^[a-z][a-z0-9-]*$/.test(value);
}

export function CloudEdit({ status: _status }: CloudEditProps) {
  const { name } = useParams<{ name: string }>();
  const [searchParams] = useSearchParams();
  const forceNew = searchParams.get("new") === "true";
  const navigate = useNavigate();

  // The URL param is either an existing provider name (editing) or a catalog kind (creating new).
  // ?new=true forces create mode (for adding a 2nd key to an existing kind).
  const [existing, setExisting] = useState<ConfiguredProvider | null>(null);
  const [resolvedKind, setResolvedKind] = useState<string>(name ?? "");
  const [existingNames, setExistingNames] = useState<string[]>([]);

  const catalog = findCatalogEntry(resolvedKind);
  const displayName = catalog?.name ?? resolvedKind ?? "Unknown";

  const [locatorName, setLocatorName] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [priority, setPriority] = useState(-10);
  const [saving, setSaving] = useState(false);
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<TestResult | null>(null);
  const [nameError, setNameError] = useState<string | null>(null);

  useEffect(() => {
    fetch("/api/providers")
      .then((r) => r.json())
      .then((data: ConfiguredProvider[]) => {
        setExistingNames(data.map((p) => p.name));

        // If forceNew, always treat as create mode (even if name matches a provider)
        if (forceNew) {
          setResolvedKind(name ?? "");
          setLocatorName("");
          return;
        }

        // Try to find by name (editing existing provider)
        const found = data.find((p) => p.name === name);
        if (found) {
          setExisting(found);
          setResolvedKind(found.kind);
          setLocatorName(found.name);
          setPriority(found.priority);
        } else {
          // Creating new: name param is the catalog kind
          setResolvedKind(name ?? "");
          setLocatorName(name ?? "");
        }
      })
      .catch(() => {
        setResolvedKind(name ?? "");
        setLocatorName(name ?? "");
      });
  }, [name, forceNew]);

  function handleNameChange(value: string) {
    const normalized = value.toLowerCase().replace(/[^a-z0-9-]/g, "");
    setLocatorName(normalized);
    if (!normalized) {
      setNameError("Name is required");
    } else if (!isValidLocatorName(normalized)) {
      setNameError("Must start with a letter, lowercase alphanumeric and hyphens only");
    } else if (!existing && existingNames.includes(normalized)) {
      setNameError(`"${normalized}" already exists — choose a different name`);
    } else {
      setNameError(null);
    }
  }

  async function handleTest() {
    if (!apiKey.trim()) return;
    setTesting(true);
    setTestResult(null);
    try {
      const resp = await fetch("/api/providers/test", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          provider: resolvedKind,
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
    if (!apiKey.trim() || !locatorName.trim()) return;
    if (nameError) return;
    setSaving(true);
    try {
      await fetch("/api/providers", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          kind: resolvedKind,
          name: locatorName.trim(),
          api_key: apiKey.trim(),
          base_url: catalog?.baseUrl ?? "",
          enabled: true,
          priority,
          capabilities: catalog?.capabilities ?? [],
          models: [],
          cached_models: testResult?.model_names ?? [],
        }),
      });
      navigate(`/infra/cloud/${locatorName.trim()}`);
    } finally {
      setSaving(false);
    }
  }

  function handleCancel() {
    navigate(existing ? `/infra/cloud/${name}` : "/infra/cloud");
  }

  const isEditing = !!existing;

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
          {existing ? (
            <Link
              to={`/infra/cloud/${name}`}
              className="text-gray-500 hover:text-gray-300 text-sm"
            >
              {displayName} / {existing.name}
            </Link>
          ) : (
            <span className="text-gray-500 text-sm">{displayName}</span>
          )}
          <span className="text-gray-600">/</span>
          <span className="text-gray-100 text-sm font-medium">Edit</span>
        </div>
        <h2 className="text-lg font-medium text-gray-100">
          {isEditing ? "Edit" : "Configure"} {displayName}
        </h2>
        {catalog && (
          <p className="text-[12px] text-gray-500 mt-1">
            {catalog.description}
          </p>
        )}
      </div>

      {/* Form */}
      <div className="bg-[#1a1b23] border border-[#2e303a] rounded-lg p-4 space-y-4">
        {/* Name (locator) field */}
        <div>
          <label className="block text-[10px] text-gray-500 mb-1 uppercase tracking-wider">
            Name (locator)
          </label>
          <input
            type="text"
            value={locatorName}
            onChange={(e) => handleNameChange(e.target.value)}
            placeholder="e.g., work, personal, staging"
            disabled={isEditing}
            className={`w-full bg-[#0f1117] border rounded px-3 py-2 text-xs text-gray-200 font-mono focus:outline-none ${
              nameError
                ? "border-red-500 focus:border-red-500"
                : "border-[#2e303a] focus:border-purple-500"
            } ${isEditing ? "opacity-60 cursor-not-allowed" : ""}`}
          />
          {nameError && (
            <p className="text-[10px] text-red-400 mt-1">{nameError}</p>
          )}
          {!nameError && !isEditing && (
            <p className="text-[10px] text-gray-600 mt-1">
              Unique identifier for this provider configuration. Use lowercase
              letters, numbers, and hyphens.
            </p>
          )}
          {isEditing && (
            <p className="text-[10px] text-gray-600 mt-1">
              Name cannot be changed after creation.
            </p>
          )}
        </div>

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
            disabled={saving || !apiKey.trim() || !locatorName.trim() || !!nameError}
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
