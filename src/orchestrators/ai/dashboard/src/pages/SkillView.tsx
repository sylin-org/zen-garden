import { useState, useEffect, useCallback } from "react";
import { useParams, useNavigate, Link } from "react-router-dom";
import { MermaidDiagram } from "../components/MermaidDiagram";
import { SkillTryIt } from "../components/SkillTryIt";
import type { SkillMapping } from "../types";

interface SkillData {
  name: string;
  display_name: string;
  description: string;
  capability: string;
  vram_mb: number;
  content_slots: Array<{ role: string; content_type: string; overlay?: string }>;
  mappings: SkillMapping[];
  required_models: Array<{ filename: string; model_type: string; url?: string; description?: string; license?: string }>;
  source?: { type: string; url?: string };
}

interface FormData {
  diagram?: string;
}

export function SkillView() {
  const { provider, moniker } = useParams<{ provider: string; moniker: string }>();
  const navigate = useNavigate();
  const [skill, setSkill] = useState<SkillData | null>(null);
  const [form, setForm] = useState<FormData | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    Promise.all([
      fetch(`/v1/services/${provider}/skills/${moniker}`).then((r) => r.json()),
      fetch(`/v1/skills/image.${moniker}/form`).then((r) => r.ok ? r.json() : null).catch(() => null),
    ]).then(([skillData, formData]) => {
      setSkill(skillData);
      setForm(formData);
      setLoading(false);
    });
  }, [provider, moniker]);

  const handleDelete = useCallback(async () => {
    if (!confirm(`Delete skill "${skill?.display_name}"?`)) return;
    await fetch(`/v1/services/${provider}/skills/${moniker}`, { method: "DELETE" });
    navigate(`/infra/services/${provider}/skills`);
  }, [provider, moniker, skill, navigate]);

  const handleClone = useCallback(async () => {
    // Read the skill, mark as draft, save with a new moniker
    if (!skill) return;
    const clone = { ...skill, draft: true, name: `${skill.name}-copy`, display_name: `${skill.display_name} (Copy)` };
    const newMoniker = `${moniker}-copy`;
    const res = await fetch(`/v1/services/${provider}/skills/${newMoniker}`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(clone),
    });
    if (res.ok) {
      navigate(`/infra/services/${provider}/skills/${newMoniker}/edit`);
    }
  }, [skill, provider, moniker, navigate]);

  if (loading || !skill) {
    return <div className="p-6 text-sm text-gray-500">Loading...</div>;
  }

  const paramMappings = skill.mappings.filter((m) => m.type === "param") as Array<Extract<SkillMapping, { type: "param" }>>;
  // Content mappings available for future use
  void skill.mappings.filter((m) => m.type === "content");

  return (
    <div className="space-y-5 max-w-6xl">
      {/* Breadcrumb */}
      <div>
        <div className="flex items-center gap-2 mb-1">
          <Link to={`/infra/services/${provider}/skills`} className="text-gray-500 hover:text-gray-300 text-sm">
            Skills
          </Link>
          <span className="text-gray-600">/</span>
          <span className="text-gray-100 text-sm font-medium">{skill.display_name}</span>
        </div>
        <div className="flex items-center gap-3">
          <h2 className="text-lg font-medium text-gray-100">{skill.display_name}</h2>
          <span className="text-[10px] px-2 py-0.5 rounded border bg-emerald-400/10 text-emerald-400 border-emerald-400/30">
            Published
          </span>
        </div>
        <p className="text-[12px] text-gray-500 mt-1">{skill.description}</p>
      </div>

      <div className="grid grid-cols-2 gap-6">
        {/* Left: Details */}
        <div className="space-y-4">
          {/* Diagram */}
          {form?.diagram && (
            <div className="bg-[#1a1b23] border border-[#2e303a] rounded-lg p-4">
              <MermaidDiagram chart={form.diagram} />
            </div>
          )}

          {/* Parameters */}
          <div className="bg-[#1a1b23] border border-[#2e303a] rounded-lg overflow-hidden">
            <div className="px-4 py-2 border-b border-[#2e303a]">
              <h3 className="text-[10px] font-semibold uppercase tracking-wider text-gray-500">
                Parameters ({paramMappings.length})
              </h3>
            </div>
            <div className="px-4 py-3 space-y-1">
              {paramMappings.map((m) => (
                <div key={m.field} className="flex items-center justify-between text-[12px] py-1">
                  <span className="text-gray-200">{m.label}</span>
                  <div className="flex items-center gap-2">
                    <span className="text-[10px] text-gray-500 font-mono">{m.param_type}</span>
                    {m.default !== undefined && m.default !== null && (
                      <span className="text-[10px] text-gray-600">= {String(m.default).slice(0, 20)}</span>
                    )}
                  </div>
                </div>
              ))}
            </div>
          </div>

          {/* Models */}
          <div className="bg-[#1a1b23] border border-[#2e303a] rounded-lg overflow-hidden">
            <div className="px-4 py-2 border-b border-[#2e303a]">
              <h3 className="text-[10px] font-semibold uppercase tracking-wider text-gray-500">
                Models ({skill.required_models.length})
              </h3>
            </div>
            <div className="px-4 py-3 space-y-2">
              {skill.required_models.map((model) => (
                <div key={model.filename} className="text-[12px]">
                  <div className="flex items-center gap-2">
                    <span className={`w-2 h-2 rounded-full ${model.url ? "bg-emerald-400" : "bg-red-400"}`} />
                    <span className="text-gray-200 font-mono">{model.filename}</span>
                  </div>
                  {model.description && (
                    <div className="ml-4 text-[10px] text-gray-500">{model.description}</div>
                  )}
                  {model.license && (
                    <div className="ml-4 text-[10px] text-amber-400/70">License: {model.license}</div>
                  )}
                </div>
              ))}
            </div>
          </div>

          {/* Source */}
          {skill.source && (
            <div className="bg-[#1a1b23] border border-[#2e303a] rounded-lg px-4 py-3">
              <div className="text-[12px] text-gray-400">
                Imported from <span className="text-gray-200">{skill.source.type}</span>
                {skill.source.url && (
                  <a href={skill.source.url} target="_blank" rel="noopener noreferrer"
                     className="ml-2 text-blue-400 hover:underline text-[11px]">
                    View source
                  </a>
                )}
              </div>
            </div>
          )}

          {/* Actions */}
          <div className="flex gap-2">
            <Link
              to={`/infra/services/${provider}/skills/${moniker}/edit`}
              className="px-4 py-2 rounded text-sm font-medium bg-blue-600 text-white hover:bg-blue-500"
            >
              Edit
            </Link>
            <button
              onClick={handleClone}
              className="px-4 py-2 rounded text-sm font-medium bg-[#2e303a] text-gray-300 hover:bg-[#3e404a]"
            >
              Clone
            </button>
            <button
              onClick={handleDelete}
              className="px-4 py-2 rounded text-sm font-medium border border-red-500/30 text-red-400 hover:bg-red-500/10"
            >
              Delete
            </button>
          </div>
        </div>

        {/* Right: Live Skill TryIt */}
        <div>
          <div className="bg-[#1a1b23] border border-[#2e303a] rounded-lg overflow-hidden">
            <div className="px-4 py-2 border-b border-[#2e303a]">
              <h3 className="text-[10px] font-semibold uppercase tracking-wider text-gray-500">Try It</h3>
            </div>
            <div className="px-4 py-3">
              <SkillTryIt skillName={`image.${moniker}`} />
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
