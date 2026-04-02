import { useState, useEffect, useCallback } from "react";
import { useParams, useNavigate, Link } from "react-router-dom";
import { MermaidDiagram } from "../components/MermaidDiagram";
import type { SkillMapping } from "../types";

interface SkillData {
  version: number;
  draft?: boolean;
  name: string;
  display_name: string;
  capability: string;
  description: string;
  provider_kind: string;
  vram_mb: number;
  default_workflow: string;
  content_slots: Array<{ role: string; content_type: string; required: boolean; overlay?: string; default?: string }>;
  mappings: SkillMapping[];
  required_models: Array<{ filename: string; model_type: string; url?: string; description?: string; license?: string }>;
  source?: { type: string; url?: string; image_id?: number };
  preview_url?: string;
  diagram?: string;
}

export function SkillEdit() {
  const { provider, moniker } = useParams<{ provider: string; moniker: string }>();
  const navigate = useNavigate();
  const [skill, setSkill] = useState<SkillData | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const isNew = moniker === "new";

  useEffect(() => {
    const endpoint = isNew
      ? `/v1/services/${provider}/skills/new`
      : `/v1/services/${provider}/skills/${moniker}`;

    fetch(endpoint)
      .then((r) => r.json())
      .then((data) => { setSkill(data); setLoading(false); })
      .catch(() => setLoading(false));
  }, [provider, moniker, isNew]);

  const updateField = useCallback((field: string, value: unknown) => {
    setSkill((prev) => prev ? { ...prev, [field]: value } : prev);
  }, []);

  const updateMapping = useCallback((idx: number, updates: Record<string, unknown>) => {
    setSkill((prev) => {
      if (!prev) return prev;
      const newMappings = prev.mappings.map((m, i) =>
        i === idx ? { ...m, ...updates } : m,
      );
      return { ...prev, mappings: newMappings };
    });
  }, []);

  const updateContentSlot = useCallback((idx: number, updates: Record<string, unknown>) => {
    setSkill((prev) => {
      if (!prev) return prev;
      const newSlots = prev.content_slots.map((s, i) =>
        i === idx ? { ...s, ...updates } : s,
      );
      return { ...prev, content_slots: newSlots };
    });
  }, []);

  const handleSave = useCallback(async () => {
    if (!skill) return;
    setSaving(true);
    setError(null);

    const saveMoniker = isNew ? (skill.name.split(".").pop() || "new-skill") : moniker;

    try {
      const res = await fetch(`/v1/services/${provider}/skills/${saveMoniker}`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(skill),
      });
      const data = await res.json();

      if (!res.ok) {
        setError(data.errors?.map((e: { message: string }) => e.message).join("\n") ?? data.error?.message ?? `HTTP ${res.status}`);
        setSaving(false);
        return;
      }
      navigate(`/infra/services/${provider}/skills/${saveMoniker}`);
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : "Save failed");
      setSaving(false);
    }
  }, [skill, provider, moniker, isNew, navigate]);

  const handleDelete = useCallback(async () => {
    if (!moniker || isNew) return;
    if (!confirm(`Delete skill "${skill?.display_name}"?`)) return;
    await fetch(`/v1/services/${provider}/skills/${moniker}`, { method: "DELETE" });
    navigate(`/infra/services/${provider}/skills`);
  }, [provider, moniker, isNew, skill, navigate]);

  if (loading || !skill) {
    return <div className="p-6 text-sm text-gray-500">Loading...</div>;
  }

  const paramMappings = skill.mappings
    .map((m, idx) => ({ mapping: m, idx }))
    .filter((x) => x.mapping.type === "param");

  return (
    <div className="space-y-5 max-w-6xl">
      {/* Breadcrumb */}
      <div>
        <div className="flex items-center gap-2 mb-1">
          <Link to={`/infra/services/${provider}/skills`} className="text-gray-500 hover:text-gray-300 text-sm">Skills</Link>
          <span className="text-gray-600">/</span>
          <span className="text-gray-100 text-sm font-medium">{isNew ? "New Skill" : skill.display_name || moniker}</span>
        </div>
        <div className="flex items-center gap-3">
          <h2 className="text-lg font-medium text-gray-100">{isNew ? "Create Skill" : "Edit Skill"}</h2>
          {skill.draft && <Badge label="Draft" color="gray" />}
        </div>
      </div>

      <div className="grid grid-cols-2 gap-6">
        {/* ── Left: Configuration ────────────────────────── */}
        <div className="space-y-4">
          {/* Metadata */}
          <Section title="Metadata">
            <Field label="Display Name" value={skill.display_name} onChange={(v) => updateField("display_name", v)} />
            <Field label="Description" value={skill.description} onChange={(v) => updateField("description", v)} multiline />
            <Field label="VRAM (MB)" value={String(skill.vram_mb)} onChange={(v) => updateField("vram_mb", parseInt(v) || 0)} type="number" />
          </Section>

          {/* Content Slots */}
          <Section title={`Content Slots (${skill.content_slots.length})`}>
            {skill.content_slots.map((slot, idx) => (
              <div key={slot.role} className="space-y-1 py-1.5 border-b border-[#2e303a]/30 last:border-0">
                <div className="flex items-center gap-2 text-[12px]">
                  <span className={`w-2 h-2 rounded-full ${slot.content_type === "image" ? "bg-blue-400" : "bg-green-400"}`} />
                  <span className="text-gray-300 font-mono">{slot.role}</span>
                  <span className="text-gray-500">{slot.content_type}</span>
                  {slot.overlay && <span className="text-[10px] text-amber-400">overlay: {slot.overlay}</span>}
                  <label className="ml-auto flex items-center gap-1 text-[10px] text-gray-500">
                    <input type="checkbox" checked={slot.required} onChange={(e) => updateContentSlot(idx, { required: e.target.checked })} />
                    required
                  </label>
                </div>
                {slot.content_type === "text" && (
                  <div className="ml-4">
                    <input
                      type="text"
                      value={slot.default ?? ""}
                      onChange={(e) => updateContentSlot(idx, { default: e.target.value || undefined })}
                      placeholder="Default value..."
                      className="w-full bg-[#0f1117] border border-[#2e303a] rounded px-2 py-1 text-[11px] text-gray-300 placeholder-gray-600 focus:outline-none focus:border-blue-500/50"
                    />
                  </div>
                )}
              </div>
            ))}
          </Section>

          {/* Parameters */}
          <Section title={`Parameters (${paramMappings.length})`}>
            {paramMappings.map(({ mapping: m, idx }) => {
              if (m.type !== "param") return null;
              return (
                <div key={m.field} className="space-y-1 py-2 border-b border-[#2e303a]/30 last:border-0">
                  <div className="flex items-center gap-2">
                    <input
                      type="text"
                      value={m.label}
                      onChange={(e) => updateMapping(idx, { label: e.target.value })}
                      className="bg-[#0f1117] border border-[#2e303a] rounded px-2 py-1 text-[12px] text-gray-200 w-32 focus:outline-none focus:border-blue-500/50"
                      title="Label"
                    />
                    <span className="text-[10px] text-gray-600 font-mono">{m.field}</span>
                    <span className="text-[10px] text-gray-500 ml-auto">{m.param_type}</span>
                    {m.node && <span className="text-[10px] text-gray-600">node:{m.node}</span>}
                  </div>
                  {/* Default value editor */}
                  {m.param_type !== "auto" && (
                    <div className="flex items-center gap-2 ml-1">
                      <span className="text-[10px] text-gray-500 w-12">Default:</span>
                      <input
                        type={m.param_type === "range" ? "number" : "text"}
                        value={m.default !== undefined && m.default !== null ? String(m.default) : ""}
                        onChange={(e) => {
                          const val = m.param_type === "range" ? parseFloat(e.target.value) || 0 : e.target.value;
                          updateMapping(idx, { default: val });
                        }}
                        className="flex-1 bg-[#0f1117] border border-[#2e303a] rounded px-2 py-0.5 text-[11px] text-gray-300 focus:outline-none focus:border-blue-500/50"
                      />
                    </div>
                  )}
                </div>
              );
            })}
            {paramMappings.length === 0 && <div className="text-[11px] text-gray-500 italic">No parameters detected.</div>}
          </Section>

          {/* Models */}
          <Section title={`Models (${skill.required_models.length})`}>
            {skill.required_models.map((model) => (
              <div key={model.filename} className="py-1.5 border-b border-[#2e303a]/30 last:border-0">
                <div className="flex items-center gap-2 text-[12px]">
                  <span className={`w-2.5 h-2.5 rounded-full ${model.url ? "bg-blue-400" : "bg-red-400"}`}
                    title={model.url ? "Resolved — URL known" : "Unresolved — needs URL"} />
                  <span className="text-gray-200 font-mono text-[11px]">{model.filename}</span>
                  <span className="text-[10px] text-gray-500 ml-auto">{model.model_type}</span>
                </div>
                {model.description && <div className="ml-5 text-[10px] text-gray-500">{model.description}</div>}
                {model.license && <div className="ml-5 text-[10px] text-amber-400/70">License: {model.license}</div>}
                {!model.url && <div className="ml-5 text-[10px] text-red-400">No download URL — provide one to enable provisioning</div>}
              </div>
            ))}
          </Section>

          {/* Source */}
          {skill.source && (
            <Section title="Source">
              <div className="text-[12px] text-gray-400">
                Imported from <span className="text-gray-200">{skill.source.type}</span>
                {skill.source.url && (
                  <a href={skill.source.url} target="_blank" rel="noopener noreferrer" className="ml-2 text-blue-400 hover:underline text-[11px]">
                    View source
                  </a>
                )}
              </div>
            </Section>
          )}

          {/* Error */}
          {error && (
            <div className="text-xs text-red-400 bg-red-400/5 border border-red-500/30 rounded px-3 py-2 whitespace-pre-wrap">{error}</div>
          )}

          {/* Actions */}
          <div className="flex gap-2 pt-2">
            <button onClick={handleSave} disabled={saving}
              className={`px-4 py-2 rounded text-sm font-medium ${saving ? "bg-gray-700 text-gray-500 cursor-not-allowed" : "bg-blue-600 text-white hover:bg-blue-500"}`}>
              {saving ? "Saving..." : skill.draft ? "Publish" : "Save"}
            </button>
            {!isNew && (
              <button onClick={handleDelete}
                className="px-4 py-2 rounded text-sm font-medium border border-red-500/30 text-red-400 hover:bg-red-500/10">
                Delete
              </button>
            )}
            <Link to={`/infra/services/${provider}/skills`}
              className="px-4 py-2 rounded text-sm font-medium text-gray-400 hover:text-gray-200">
              Cancel
            </Link>
          </div>
        </div>

        {/* ── Right: Preview ─────────────────────────────── */}
        <div className="space-y-4">
          {/* Preview image */}
          {skill.preview_url && (
            <Section title="Preview Image">
              <img
                src={skill.preview_url}
                alt="Generated preview"
                className="w-full rounded border border-gray-700 max-h-80 object-contain bg-black"
                onError={(e) => { (e.target as HTMLImageElement).style.display = "none"; }}
              />
            </Section>
          )}

          {/* Diagram */}
          {skill.diagram && (
            <Section title="Workflow Diagram">
              <MermaidDiagram chart={skill.diagram} />
            </Section>
          )}

          {/* Workflow files */}
          <Section title="Workflow Templates">
            <div className="text-[12px] text-gray-400">
              <div className="flex items-center gap-2">
                <span className="w-2 h-2 rounded-full bg-emerald-400" />
                <span className="font-mono">{skill.default_workflow}.json</span>
                <span className="text-[10px] text-emerald-400">default</span>
              </div>
            </div>
          </Section>
        </div>
      </div>
    </div>
  );
}

// ── Shared Components ─────────────────────────────────────────

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div className="bg-[#1a1b23] border border-[#2e303a] rounded-lg overflow-hidden">
      <div className="px-4 py-2 border-b border-[#2e303a]">
        <h3 className="text-[10px] font-semibold uppercase tracking-wider text-gray-500">{title}</h3>
      </div>
      <div className="px-4 py-3 space-y-2">{children}</div>
    </div>
  );
}

function Badge({ label, color }: { label: string; color: "gray" | "green" | "blue" }) {
  const styles = {
    gray: "bg-gray-600/10 text-gray-500 border-gray-600/30",
    green: "bg-emerald-400/10 text-emerald-400 border-emerald-400/30",
    blue: "bg-blue-400/10 text-blue-400 border-blue-400/30",
  };
  return <span className={`text-[10px] px-2 py-0.5 rounded border ${styles[color]}`}>{label}</span>;
}

function Field({ label, value, onChange, type = "text", multiline }: {
  label: string; value: string; onChange: (v: string) => void; type?: string; multiline?: boolean;
}) {
  return (
    <div className="flex items-start gap-3">
      <label className="text-[11px] text-gray-500 w-24 shrink-0 pt-1">{label}</label>
      {multiline ? (
        <textarea
          value={value}
          onChange={(e) => onChange(e.target.value)}
          rows={3}
          className="flex-1 bg-[#0f1117] border border-[#2e303a] rounded px-2 py-1 text-[12px] text-gray-200 focus:outline-none focus:border-blue-500/50 resize-y"
        />
      ) : (
        <input
          type={type}
          value={value}
          onChange={(e) => onChange(e.target.value)}
          className="flex-1 bg-[#0f1117] border border-[#2e303a] rounded px-2 py-1 text-[12px] text-gray-200 focus:outline-none focus:border-blue-500/50"
        />
      )}
    </div>
  );
}
