import { useState, useEffect, useCallback, useRef } from "react";
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

  useEffect(() => {
    fetchSkills();

    // Subscribe to SSE for live updates (skill.named, etc.)
    const es = new EventSource("/api/events");
    es.addEventListener("skill.named", (e: MessageEvent) => {
      try {
        const data = JSON.parse(e.data);
        setSkills((prev) =>
          prev.map((s) => {
            const moniker = s.name.split(".").slice(1).join(".") || s.name;
            if (moniker === data.moniker || s.name === data.skill) {
              return { ...s, display_name: data.display_name, description: data.description };
            }
            return s;
          })
        );
      } catch { /* ignore parse errors */ }
    });

    return () => es.close();
  }, [fetchSkills]);

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
  const [renaming, setRenaming] = useState(false);
  const nameRef = useRef(skill.display_name);

  // Update displayed name when SSE pushes a new one
  const displayName = skill.display_name || moniker;
  if (nameRef.current !== skill.display_name) {
    nameRef.current = skill.display_name;
  }

  const handleRename = useCallback(async (e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setRenaming(true);
    try {
      await fetch(`/v1/services/${provider}/skills/${moniker}/rename`, { method: "POST" });
      // SSE event will update the name — no need to handle response
    } catch { /* ignore */ }
    // Keep the spinner for a moment so the user sees feedback
    setTimeout(() => setRenaming(false), 2000);
  }, [provider, moniker]);

  return (
    <div className="bg-[#1a1b23] border border-[#2e303a] rounded-lg px-4 py-3 hover:bg-[#1e1f28] transition-colors">
      <div className="flex items-center justify-between">
        <Link
          to={isDraft
            ? `/infra/services/${provider}/skills/${moniker}/edit`
            : `/infra/services/${provider}/skills/${moniker}`
          }
          className="flex items-center gap-3 flex-1 min-w-0"
        >
          <span
            className={`w-2.5 h-2.5 rounded-full flex-shrink-0 ${
              isDraft ? "bg-gray-500" : "bg-emerald-400"
            }`}
          />
          <div className="min-w-0">
            <span className="text-sm font-medium text-gray-100">
              {displayName}
            </span>
            <span className="ml-2 text-[11px] text-gray-500 truncate">
              {skill.description?.length > 80
                ? `${skill.description.slice(0, 80)}...`
                : skill.description}
            </span>
          </div>
        </Link>
        <div className="flex items-center gap-2 flex-shrink-0">
          <button
            onClick={handleRename}
            disabled={renaming}
            className="text-[10px] px-2 py-0.5 rounded border border-blue-500/30 text-blue-400 hover:bg-blue-500/10 transition-colors disabled:opacity-50"
            title="Rename with AI"
          >
            {renaming ? "Naming..." : "AI Name"}
          </button>
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
    </div>
  );
}
