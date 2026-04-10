import { useEffect, useState, useCallback } from "react";
import { get, put, del } from "../../api/client";
import { useSSE } from "../../hooks/useSSE";

type Preferences = Record<string, unknown>;

export default function PreferenceEditor() {
  const [prefs, setPrefs] = useState<Preferences>({});
  const [loading, setLoading] = useState(true);
  const [newKey, setNewKey] = useState("");
  const [newValue, setNewValue] = useState("");

  const fetchPrefs = useCallback(async () => {
    try {
      const data = await get<Preferences>("/v1/preferences");
      setPrefs(data);
    } catch { /* non-fatal */ }
    setLoading(false);
  }, []);

  useEffect(() => { fetchPrefs(); }, [fetchPrefs]);

  useSSE({
    focus: "preferences.changed",
    onEvent: () => { fetchPrefs(); },
  });

  const handleRemove = useCallback(async (key: string) => {
    await del(`/v1/preferences/${key}`);
    fetchPrefs();
  }, [fetchPrefs]);

  const handleAdd = useCallback(async () => {
    if (!newKey.trim()) return;
    let parsed: unknown;
    try {
      parsed = JSON.parse(newValue);
    } catch {
      parsed = newValue;
    }
    await put("/v1/preferences", { [newKey]: parsed });
    setNewKey("");
    setNewValue("");
    fetchPrefs();
  }, [newKey, newValue, fetchPrefs]);

  if (loading) {
    return <div className="p-4 text-text-dimmer text-sm">Loading...</div>;
  }

  const entries = Object.entries(prefs);

  return (
    <div className="p-5 overflow-y-auto h-full">
      <h3 className="text-sm font-semibold mb-4">Preferences</h3>
      <p className="text-[11px] text-text-dim mb-4">
        Global defaults that auto-populate form fields. Caller payloads override these.
      </p>

      {/* Existing preferences */}
      <div className="space-y-1 mb-6">
        {entries.length === 0 ? (
          <div className="text-[11px] text-text-dimmer italic">No preferences set</div>
        ) : (
          entries.map(([key, value]) => (
            <div key={key} className="flex items-center gap-3 py-2 border-b border-border group">
              <span className="text-[12px] font-mono text-accent flex-1">{key}</span>
              <span className="text-[12px] text-text font-medium">{JSON.stringify(value)}</span>
              <button
                onClick={() => handleRemove(key)}
                className="text-[10px] text-text-dimmer hover:text-red opacity-0 group-hover:opacity-100 transition-opacity"
              >
                Reset
              </button>
            </div>
          ))
        )}
      </div>

      {/* Add preference */}
      <div className="flex gap-2 items-end">
        <div className="flex-1">
          <label className="block text-[10px] text-text-dimmer mb-1">Field path</label>
          <input
            type="text"
            placeholder="image.width"
            className="w-full px-2.5 py-1.5 bg-surface-2 border border-border rounded text-[11px] text-text
                       placeholder:text-text-dimmer outline-none focus:border-accent font-mono"
            value={newKey}
            onChange={(e) => setNewKey(e.target.value)}
          />
        </div>
        <div className="flex-1">
          <label className="block text-[10px] text-text-dimmer mb-1">Value</label>
          <input
            type="text"
            placeholder="1024"
            className="w-full px-2.5 py-1.5 bg-surface-2 border border-border rounded text-[11px] text-text
                       placeholder:text-text-dimmer outline-none focus:border-accent"
            value={newValue}
            onChange={(e) => setNewValue(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && handleAdd()}
          />
        </div>
        <button
          onClick={handleAdd}
          className="px-3 py-1.5 bg-accent hover:bg-accent-dim text-white text-[11px] font-semibold rounded transition-colors"
        >
          Add
        </button>
      </div>
    </div>
  );
}
