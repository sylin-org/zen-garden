import { useState, useEffect, useCallback } from "react";
import { useParams, useNavigate, Link } from "react-router-dom";
import { MermaidDiagram } from "../components/MermaidDiagram";
import { SkillTryIt } from "../components/SkillTryIt";
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
  content_slots: Array<{ role: string; content_type: string; required: boolean; overlay?: string }>;
  mappings: SkillMapping[];
  required_models: Array<{ filename: string; model_type: string; url?: string; description?: string }>;
  source?: { type: string; url?: string; image_id?: number };
}

export function SkillEdit() {
  const { provider, moniker } = useParams<{ provider: string; moniker: string }>();
  const navigate = useNavigate();
  const [skill, setSkill] = useState<SkillData | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [diagram, setDiagram] = useState<string | null>(null);

  const isNew = moniker === "new";

  useEffect(() => {
    const endpoint = isNew
      ? `/v1/services/${provider}/skills/new`
      : `/v1/services/${provider}/skills/${moniker}`;

    fetch(endpoint)
      .then((r) => r.json())
      .then((data) => {
        setSkill(data);
        setLoading(false);
      })
      .catch(() => setLoading(false));
  }, [provider, moniker, isNew]);

  // Fetch diagram from form endpoint if this is an existing skill
  useEffect(() => {
    if (!isNew && moniker) {
      fetch(`/v1/skills/image.${moniker}/form`)
        .then((r) => r.ok ? r.json() : null)
        .then((data) => {
          if (data?.diagram) setDiagram(data.diagram);
        })
        .catch(() => {});
    }
  }, [moniker, isNew]);

  const updateField = useCallback((field: string, value: unknown) => {
    setSkill((prev) => prev ? { ...prev, [field]: value } : prev);
  }, []);

  const handleSave = useCallback(async () => {
    if (!skill) return;
    setSaving(true);
    setError(null);

    const saveMoniker = isNew
      ? skill.name.split(".").pop() || "new-skill"
      : moniker;

    try {
      const res = await fetch(`/v1/services/${provider}/skills/${saveMoniker}`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(skill),
      });
      const data = await res.json();

      if (!res.ok) {
        if (data.errors) {
          setError(data.errors.map((e: { message: string }) => e.message).join("\n"));
        } else {
          setError(data.error?.message ?? `HTTP ${res.status}`);
        }
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

  const paramMappings = skill.mappings.filter((m) => m.type === "param") as Array<Extract<SkillMapping, { type: "param" }>>;

  return (
    <div className="space-y-5 max-w-6xl">
      {/* Breadcrumb */}
      <div>
        <div className="flex items-center gap-2 mb-1">
          <Link to={`/infra/services/${provider}/skills`} className="text-gray-500 hover:text-gray-300 text-sm">
            Skills
          </Link>
          <span className="text-gray-600">/</span>
          <span className="text-gray-100 text-sm font-medium">
            {isNew ? "New Skill" : skill.display_name || moniker}
          </span>
        </div>
        <div className="flex items-center gap-3">
          <h2 className="text-lg font-medium text-gray-100">
            {isNew ? "Create Skill" : "Edit Skill"}
          </h2>
          {skill.draft && (
            <span className="text-[10px] px-2 py-0.5 rounded border bg-gray-600/10 text-gray-500 border-gray-600/30">
              Draft
            </span>
          )}
        </div>
      </div>

      <div className="grid grid-cols-2 gap-6">
        {/* Left: Configuration */}
        <div className="space-y-4">
          {/* Metadata */}
          <Section title="Metadata">
            <Field label="Display Name" value={skill.display_name}
              onChange={(v) => updateField("display_name", v)} />
            <Field label="Description" value={skill.description}
              onChange={(v) => updateField("description", v)} />
            <Field label="VRAM (MB)" value={String(skill.vram_mb)}
              onChange={(v) => updateField("vram_mb", parseInt(v) || 0)} type="number" />
          </Section>

          {/* Content Slots */}
          <Section title={`Content Slots (${skill.content_slots.length})`}>
            {skill.content_slots.map((slot) => (
              <div key={slot.role} className="flex items-center gap-2 text-[12px]">
                <span className={`w-2 h-2 rounded-full ${slot.content_type === "image" ? "bg-blue-400" : "bg-green-400"}`} />
                <span className="text-gray-300 font-mono">{slot.role}</span>
                <span className="text-gray-500">{slot.content_type}</span>
                {slot.overlay && <span className="text-[10px] text-amber-400">overlay: {slot.overlay}</span>}
              </div>
            ))}
          </Section>

          {/* Parameters */}
          <Section title={`Parameters (${paramMappings.length})`}>
            {paramMappings.map((m) => (
              <div key={m.field} className="flex items-center justify-between text-[12px] py-1 border-b border-[#2e303a]/30">
                <div>
                  <span className="text-gray-200">{m.label}</span>
                  <span className="ml-2 text-gray-600 font-mono text-[10px]">{m.field}</span>
                </div>
                <div className="flex items-center gap-2">
                  <span className="text-[10px] text-gray-500">{m.param_type}</span>
                  {m.node && <span className="text-[10px] text-gray-600">node:{m.node}</span>}
                </div>
              </div>
            ))}
            {paramMappings.length === 0 && (
              <div className="text-[11px] text-gray-500 italic">No parameters detected.</div>
            )}
          </Section>

          {/* Models */}
          <Section title={`Models (${skill.required_models.length})`}>
            {skill.required_models.map((model) => (
              <div key={model.filename} className="flex items-center justify-between text-[12px] py-1">
                <div className="flex items-center gap-2">
                  <span className={`w-2 h-2 rounded-full ${model.url ? "bg-emerald-400" : "bg-red-400"}`} />
                  <span className="text-gray-300 font-mono">{model.filename}</span>
                </div>
                <span className="text-[10px] text-gray-500">{model.model_type}</span>
              </div>
            ))}
            {skill.required_models.length === 0 && (
              <div className="text-[11px] text-gray-500 italic">No models required.</div>
            )}
          </Section>

          {/* Source */}
          {skill.source && (
            <Section title="Source">
              <div className="text-[12px] text-gray-400">
                Imported from {skill.source.type}
                {skill.source.url && (
                  <a href={skill.source.url} target="_blank" rel="noopener noreferrer"
                     className="ml-2 text-blue-400 hover:underline">
                    View source
                  </a>
                )}
              </div>
            </Section>
          )}

          {/* Error */}
          {error && (
            <div className="text-xs text-red-400 bg-red-400/5 border border-red-500/30 rounded px-3 py-2 whitespace-pre-wrap">
              {error}
            </div>
          )}

          {/* Actions */}
          <div className="flex gap-2 pt-2">
            <button
              onClick={handleSave}
              disabled={saving}
              className={`px-4 py-2 rounded text-sm font-medium ${
                saving
                  ? "bg-gray-700 text-gray-500 cursor-not-allowed"
                  : "bg-blue-600 text-white hover:bg-blue-500"
              }`}
            >
              {saving ? "Saving..." : skill.draft ? "Publish" : "Save"}
            </button>
            {!isNew && (
              <button
                onClick={handleDelete}
                className="px-4 py-2 rounded text-sm font-medium border border-red-500/30 text-red-400 hover:bg-red-500/10"
              >
                Delete
              </button>
            )}
            <Link
              to={`/infra/services/${provider}/skills`}
              className="px-4 py-2 rounded text-sm font-medium text-gray-400 hover:text-gray-200"
            >
              Cancel
            </Link>
          </div>
        </div>

        {/* Right: Live Preview */}
        <div className="space-y-4">
          <Section title="Preview">
            {diagram && (
              <div className="bg-[#0d0e14] rounded px-3 py-2 border border-gray-800 mb-3">
                <MermaidDiagram chart={diagram} />
              </div>
            )}

            {!isNew && !skill.draft && moniker && (
              <div className="bg-[#0d0e14] rounded border border-gray-800 p-3">
                <div className="text-[10px] text-gray-500 uppercase tracking-wider mb-2">Live Form Preview</div>
                <SkillTryIt skillName={`image.${moniker}`} disabled />
              </div>
            )}

            {(isNew || skill.draft) && skill.mappings.length > 0 && (
              <div className="bg-[#0d0e14] rounded border border-gray-800 p-3">
                <div className="text-[10px] text-gray-500 uppercase tracking-wider mb-2">Form Preview (draft)</div>
                <div className="text-[11px] text-gray-500 italic">
                  Save to see the live form preview.
                </div>
              </div>
            )}
          </Section>
        </div>
      </div>
    </div>
  );
}

// ── Helper Components ─────────────────────────────────────────

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

function Field({
  label,
  value,
  onChange,
  type = "text",
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
  type?: string;
}) {
  return (
    <div className="flex items-center gap-3">
      <label className="text-[11px] text-gray-500 w-24 shrink-0">{label}</label>
      <input
        type={type}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        className="flex-1 bg-[#0f1117] border border-[#2e303a] rounded px-2 py-1 text-[12px] text-gray-200 focus:outline-none focus:border-blue-500/50"
      />
    </div>
  );
}
