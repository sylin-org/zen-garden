import { useState, useEffect, useCallback } from "react";
import { useLocation } from "react-router-dom";

interface SecretEntry {
  key: string;
  label: string;
  description: string;
  is_set: boolean;
  masked_value?: string;
}

export function Secrets() {
  const [secrets, setSecrets] = useState<SecretEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [editingKey, setEditingKey] = useState<string | null>(null);
  const [editValue, setEditValue] = useState("");
  const [saving, setSaving] = useState(false);
  const location = useLocation();

  const fetchSecrets = useCallback(async () => {
    const res = await fetch("/v1/secrets");
    if (res.ok) setSecrets(await res.json());
    setLoading(false);
  }, []);

  useEffect(() => { fetchSecrets(); }, [fetchSecrets]);

  // Auto-focus on a specific key if hash is present (e.g., #civitai)
  useEffect(() => {
    if (location.hash) {
      const key = location.hash.replace("#", "");
      setEditingKey(key);
      setTimeout(() => {
        document.getElementById(`secret-input-${key}`)?.focus();
      }, 100);
    }
  }, [location.hash, secrets]);

  const handleSave = useCallback(async (key: string) => {
    setSaving(true);
    await fetch(`/v1/secrets/${key}`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ value: editValue }),
    });
    setEditingKey(null);
    setEditValue("");
    setSaving(false);
    fetchSecrets();
  }, [editValue, fetchSecrets]);

  const handleDelete = useCallback(async (key: string) => {
    if (!confirm(`Delete the ${key} API key?`)) return;
    await fetch(`/v1/secrets/${key}`, { method: "DELETE" });
    fetchSecrets();
  }, [fetchSecrets]);

  if (loading) {
    return <div className="p-6 text-sm text-gray-500">Loading...</div>;
  }

  return (
    <div className="space-y-5 max-w-3xl">
      <div>
        <h2 className="text-lg font-medium text-gray-100">Secrets</h2>
        <p className="text-[12px] text-gray-500 mt-1">
          API keys for external services. Stored locally, never transmitted.
        </p>
      </div>

      <div className="space-y-3">
        {secrets.map((secret) => (
          <div
            key={secret.key}
            id={`secret-${secret.key}`}
            className="bg-[#1a1b23] border border-[#2e303a] rounded-lg px-4 py-3"
          >
            <div className="flex items-center justify-between mb-1">
              <div>
                <span className="text-sm font-medium text-gray-100">{secret.label}</span>
                <span className={`ml-2 text-[10px] px-1.5 py-0.5 rounded ${
                  secret.is_set
                    ? "bg-emerald-400/10 text-emerald-400 border border-emerald-400/30"
                    : "bg-gray-600/10 text-gray-500 border border-gray-600/30"
                }`}>
                  {secret.is_set ? "Configured" : "Not set"}
                </span>
              </div>
              <div className="flex items-center gap-2">
                {secret.is_set && editingKey !== secret.key && (
                  <span className="text-[11px] text-gray-500 font-mono">{secret.masked_value}</span>
                )}
                {editingKey !== secret.key && (
                  <button
                    onClick={() => { setEditingKey(secret.key); setEditValue(""); }}
                    className="text-[11px] px-2 py-0.5 rounded bg-[#2e303a] text-gray-300 hover:bg-[#3e404a]"
                  >
                    {secret.is_set ? "Change" : "Set"}
                  </button>
                )}
                {secret.is_set && editingKey !== secret.key && (
                  <button
                    onClick={() => handleDelete(secret.key)}
                    className="text-[11px] px-2 py-0.5 rounded border border-red-500/30 text-red-400 hover:bg-red-500/10"
                  >
                    Remove
                  </button>
                )}
              </div>
            </div>

            <p className="text-[11px] text-gray-500">{secret.description}</p>

            {editingKey === secret.key && (
              <div className="flex items-center gap-2 mt-2">
                <input
                  id={`secret-input-${secret.key}`}
                  type="password"
                  value={editValue}
                  onChange={(e) => setEditValue(e.target.value)}
                  placeholder="Paste your API key..."
                  className="flex-1 bg-[#0f1117] border border-[#2e303a] rounded px-3 py-1.5 text-sm text-gray-200 placeholder-gray-600 focus:outline-none focus:border-blue-500/50 font-mono"
                  onKeyDown={(e) => { if (e.key === "Enter" && editValue) handleSave(secret.key); }}
                  autoFocus
                />
                <button
                  onClick={() => handleSave(secret.key)}
                  disabled={saving || !editValue}
                  className={`px-3 py-1.5 rounded text-sm font-medium ${
                    saving || !editValue
                      ? "bg-gray-700 text-gray-500 cursor-not-allowed"
                      : "bg-blue-600 text-white hover:bg-blue-500"
                  }`}
                >
                  {saving ? "Saving..." : "Save"}
                </button>
                <button
                  onClick={() => { setEditingKey(null); setEditValue(""); }}
                  className="text-sm text-gray-400 hover:text-gray-200"
                >
                  Cancel
                </button>
              </div>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}
