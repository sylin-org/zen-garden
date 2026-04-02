import { useState, useEffect, useCallback } from "react";
import { useParams, Link, useNavigate } from "react-router-dom";

interface SkillEntry {
  name: string;
  display_name: string;
  draft?: boolean;
  capability: string;
  description: string;
  required_models: Array<{ filename: string }>;
  source?: { type: string; url?: string; image_id?: number };
}

export function SkillsList() {
  const { provider } = useParams<{ provider: string }>();
  const navigate = useNavigate();
  const [skills, setSkills] = useState<SkillEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [importInput, setImportInput] = useState("");
  const [importing, setImporting] = useState(false);
  const [importError, setImportError] = useState<string | null>(null);

  const fetchSkills = useCallback(async () => {
    const res = await fetch(`/v1/services/${provider}/skills`);
    if (res.ok) {
      const data = await res.json();
      setSkills(data);
    }
    setLoading(false);
  }, [provider]);

  useEffect(() => { fetchSkills(); }, [fetchSkills]);

  const handleImport = useCallback(async () => {
    if (!importInput.trim()) return;
    setImporting(true);
    setImportError(null);

    try {
      const res = await fetch(
        `/v1/services/${provider}/skills/analyze?t=${encodeURIComponent(importInput.trim())}`,
      );
      const data = await res.json();

      if (!res.ok) {
        setImportError(data.error?.message ?? `HTTP ${res.status}`);
        setImporting(false);
        return;
      }

      // Navigate to the draft in edit mode
      navigate(`/infra/services/${provider}/skills/${data.moniker}/edit`);
    } catch (err: unknown) {
      setImportError(err instanceof Error ? err.message : "Import failed");
      setImporting(false);
    }
  }, [importInput, provider, navigate]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === "Enter") handleImport();
    },
    [handleImport],
  );

  if (loading) {
    return <div className="p-6 text-sm text-gray-500">Loading skills...</div>;
  }

  const publishedSkills = skills.filter((s) => !s.draft);
  const draftSkills = skills.filter((s) => s.draft);

  return (
    <div className="space-y-5 max-w-4xl">
      {/* Breadcrumb */}
      <div>
        <div className="flex items-center gap-2 mb-1">
          <Link to="/infra/services" className="text-gray-500 hover:text-gray-300 text-sm">
            Services
          </Link>
          <span className="text-gray-600">/</span>
          <Link to={`/infra/services/${provider}`} className="text-gray-500 hover:text-gray-300 text-sm">
            {provider}
          </Link>
          <span className="text-gray-600">/</span>
          <span className="text-gray-100 text-sm font-medium">Skills</span>
        </div>
        <h2 className="text-lg font-medium text-gray-100">Skill Management</h2>
      </div>

      {/* Smart import input */}
      <div className="bg-[#1a1b23] border border-[#2e303a] rounded-lg p-4 space-y-3">
        <div className="flex gap-2">
          <input
            type="text"
            value={importInput}
            onChange={(e) => setImportInput(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="Paste a CivitAI URL, PNG URL, or workflow JSON to import..."
            className="flex-1 bg-[#0f1117] border border-[#2e303a] rounded px-3 py-2 text-sm text-gray-200 placeholder-gray-600 focus:outline-none focus:border-blue-500/50"
            disabled={importing}
          />
          <button
            onClick={handleImport}
            disabled={importing || !importInput.trim()}
            className={`px-4 py-2 rounded text-sm font-medium ${
              importing || !importInput.trim()
                ? "bg-gray-700 text-gray-500 cursor-not-allowed"
                : "bg-blue-600 text-white hover:bg-blue-500"
            }`}
          >
            {importing ? "Analyzing..." : "Import"}
          </button>
          <Link
            to={`/infra/services/${provider}/skills/new/edit`}
            className="px-4 py-2 rounded text-sm font-medium bg-[#2e303a] text-gray-300 hover:bg-[#3e404a]"
          >
            New
          </Link>
        </div>
        {importError && (
          <div className="text-xs text-red-400 bg-red-400/5 border border-red-500/30 rounded px-3 py-2">
            {importError}
          </div>
        )}
      </div>

      {/* Published skills */}
      <div className="space-y-2">
        <h3 className="text-[10px] font-semibold uppercase tracking-wider text-gray-500">
          Published Skills ({publishedSkills.length})
        </h3>
        {publishedSkills.length === 0 ? (
          <div className="text-sm text-gray-500 italic">No published skills yet.</div>
        ) : (
          publishedSkills.map((skill) => (
            <SkillRow key={skill.name} skill={skill} provider={provider!} />
          ))
        )}
      </div>

      {/* Draft skills */}
      {draftSkills.length > 0 && (
        <div className="space-y-2">
          <h3 className="text-[10px] font-semibold uppercase tracking-wider text-gray-500">
            Drafts ({draftSkills.length})
          </h3>
          {draftSkills.map((skill) => (
            <SkillRow key={skill.name} skill={skill} provider={provider!} isDraft />
          ))}
        </div>
      )}
    </div>
  );
}

function SkillRow({
  skill,
  provider,
  isDraft,
}: {
  skill: SkillEntry;
  provider: string;
  isDraft?: boolean;
}) {
  const moniker = skill.name.split(".").slice(1).join(".") || skill.name;

  return (
    <Link
      to={isDraft
        ? `/infra/services/${provider}/skills/${moniker}/edit`
        : `/infra/services/${provider}/skills/${moniker}`
      }
      className="block bg-[#1a1b23] border border-[#2e303a] rounded-lg px-4 py-3 hover:bg-[#1e1f28] transition-colors"
    >
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <span
            className={`w-2.5 h-2.5 rounded-full ${
              isDraft ? "bg-gray-500" : "bg-emerald-400"
            }`}
          />
          <div>
            <span className="text-sm font-medium text-gray-100">
              {skill.display_name || moniker}
            </span>
            <span className="ml-2 text-[11px] text-gray-500">
              {skill.description}
            </span>
          </div>
        </div>
        <div className="flex items-center gap-2">
          {isDraft && (
            <span className="text-[10px] px-2 py-0.5 rounded border bg-gray-600/10 text-gray-500 border-gray-600/30">
              Draft
            </span>
          )}
          {skill.source?.type === "civitai" && (
            <span className="text-[10px] text-purple-400">CivitAI</span>
          )}
          <span className="text-[10px] text-gray-600">
            {skill.required_models?.length ?? 0} model{(skill.required_models?.length ?? 0) !== 1 ? "s" : ""}
          </span>
        </div>
      </div>
    </Link>
  );
}
