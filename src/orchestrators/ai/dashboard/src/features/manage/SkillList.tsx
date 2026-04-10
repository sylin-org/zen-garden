import { useEffect, useState, useCallback } from "react";
import { get } from "../../api/client";
import type { SkillListResponse, SkillView, WorkspaceSpec } from "../../api/types";
import { useSSE } from "../../hooks/useSSE";
import WorkspaceForm from "../create/WorkspaceForm";
import ResultPanel from "../create/ResultPanel";

export default function SkillList() {
  const [skills, setSkills] = useState<SkillView[]>([]);
  const [selected, setSelected] = useState<SkillView | null>(null);
  const [loading, setLoading] = useState(true);
  const [filter, setFilter] = useState("");

  // Workspace spec for the selected skill
  const [spec, setSpec] = useState<WorkspaceSpec | null>(null);
  const [specLoading, setSpecLoading] = useState(false);
  const [result, setResult] = useState<unknown>(null);

  const fetchSkills = async () => {
    try {
      const data = await get<SkillListResponse>("/v1/skills");
      setSkills(data.skills);
    } catch { /* non-fatal */ }
    setLoading(false);
  };

  useEffect(() => { fetchSkills(); }, []);

  useSSE({
    focus: "skills.*,catalog.version",
    onEvent: () => { fetchSkills(); },
  });

  // Fetch workspace spec when a skill is selected
  useEffect(() => {
    if (!selected) {
      setSpec(null);
      setResult(null);
      return;
    }
    setSpecLoading(true);
    const url = `/v1/${selected.primitive.replace(/\./g, "/")}/${selected.id}`;
    get<WorkspaceSpec>(url)
      .then((s) => { setSpec(s); setSpecLoading(false); })
      .catch(() => { setSpec(null); setSpecLoading(false); });
  }, [selected]);

  const handleResult = useCallback((r: unknown) => { setResult(r); }, []);
  const handleError = useCallback((e: unknown) => { setResult(e); }, []);

  const filtered = skills.filter((s) =>
    !filter || s.display.name.toLowerCase().includes(filter.toLowerCase())
      || s.id.toLowerCase().includes(filter.toLowerCase())
      || s.primitive.includes(filter.toLowerCase()),
  );

  const hasResult = result != null;

  return (
    <div className="flex h-full">
      {/* Master: skill list */}
      <div
        className="flex flex-col overflow-hidden border-r border-border transition-all duration-300"
        style={{ flexBasis: selected ? (hasResult ? "25%" : "35%") : "100%", minWidth: "200px" }}
      >
        {/* Search */}
        <div className="p-3 border-b border-border shrink-0">
          <input
            type="text"
            placeholder="Filter skills..."
            className="w-full px-3 py-1.5 bg-surface-2 border border-border rounded text-[12px] text-text
                       placeholder:text-text-dimmer outline-none focus:border-accent"
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
          />
        </div>

        {/* List */}
        <div className="flex-1 overflow-y-auto">
          {loading ? (
            <div className="p-4 text-text-dimmer text-sm">Loading...</div>
          ) : filtered.length === 0 ? (
            <div className="p-4 text-text-dimmer text-sm italic">No skills found</div>
          ) : (
            filtered.map((skill) => (
              <div
                key={skill.id}
                onClick={() => setSelected(skill)}
                className={[
                  "flex items-center gap-3 px-4 py-2.5 cursor-pointer border-b border-border transition-colors",
                  selected?.id === skill.id
                    ? "bg-accent-bg"
                    : "hover:bg-surface",
                ].join(" ")}
              >
                <div className="flex-1 min-w-0">
                  <div className="text-[12px] font-medium truncate">{skill.display.name}</div>
                  <div className="text-[10px] text-text-dimmer">{skill.primitive} · {skill.provider}</div>
                </div>
              </div>
            ))
          )}
        </div>

        <div className="px-4 py-2 border-t border-border text-[10px] text-text-dimmer shrink-0">
          {skills.length} skills
        </div>
      </div>

      {/* Detail: workspace form for the selected skill */}
      {selected && (
        <div
          className="border-l border-border overflow-hidden bg-surface transition-all duration-300 flex"
          style={{ flexBasis: hasResult ? "75%" : "65%" }}
        >
          {/* Form */}
          <div className="flex-1 overflow-hidden">
            {specLoading ? (
              <div className="flex items-center justify-center h-full text-text-dim text-sm">
                Loading workspace...
              </div>
            ) : spec ? (
              <WorkspaceForm
                key={selected.id}
                spec={spec}
                onResult={handleResult}
                onError={handleError}
              />
            ) : (
              <div className="flex items-center justify-center h-full text-text-dimmer text-sm italic">
                Failed to load workspace
              </div>
            )}
          </div>

          {/* Result panel — appears when there's a result */}
          {hasResult && (
            <div className="w-[340px] shrink-0 border-l border-border overflow-hidden">
              <ResultPanel result={result} />
            </div>
          )}
        </div>
      )}
    </div>
  );
}
