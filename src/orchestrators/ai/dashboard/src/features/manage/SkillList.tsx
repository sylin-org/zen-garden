import { useEffect, useState } from "react";
import { get } from "../../api/client";
import type { SkillListResponse, SkillView } from "../../api/types";
import { useSSE } from "../../hooks/useSSE";

export default function SkillList() {
  const [skills, setSkills] = useState<SkillView[]>([]);
  const [selected, setSelected] = useState<SkillView | null>(null);
  const [loading, setLoading] = useState(true);
  const [filter, setFilter] = useState("");

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

  const filtered = skills.filter((s) =>
    !filter || s.display.name.toLowerCase().includes(filter.toLowerCase())
      || s.id.toLowerCase().includes(filter.toLowerCase())
      || s.primitive.includes(filter.toLowerCase()),
  );

  return (
    <div className="flex h-full">
      {/* Master: skill list */}
      <div className="flex-1 flex flex-col overflow-hidden border-r border-border">
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

      {/* Detail */}
      <div className="w-[380px] shrink-0 overflow-y-auto bg-surface">
        {selected ? (
          <SkillDetail skill={selected} />
        ) : (
          <div className="flex items-center justify-center h-full text-text-dimmer text-xs italic">
            Select a skill
          </div>
        )}
      </div>
    </div>
  );
}

function SkillDetail({ skill }: { skill: SkillView }) {
  return (
    <div className="p-5">
      <h3 className="text-sm font-semibold mb-1">{skill.display.name}</h3>
      <div className="text-[11px] text-text-dim mb-4">{skill.id}</div>

      {skill.display.description && (
        <div className="text-[12px] text-text-dim mb-4 leading-relaxed">
          {skill.display.description}
        </div>
      )}

      <Section label="Info">
        <KV k="Primitive" v={skill.primitive} />
        <KV k="Provider" v={skill.provider} />
        <KV k="Parameters" v={String(skill.parameters.length)} />
      </Section>

      {skill.parameters.length > 0 && (
        <Section label="Parameters">
          {skill.parameters.map((p) => (
            <div key={p.field} className="py-1">
              <div className="text-[11px] font-mono text-accent">{p.field}</div>
              <div className="text-[10px] text-text-dimmer">
                {p.required ? "required" : "optional"}
                {p.default !== undefined && p.default !== null
                  ? ` · default: ${JSON.stringify(p.default)}`
                  : ""}
              </div>
            </div>
          ))}
        </Section>
      )}

      <div className="mt-4">
        <a
          href={`/create/${skill.primitive.replace(/\./g, "/")}/${skill.id}`}
          className="text-[11px] text-accent hover:underline"
        >
          Try it →
        </a>
      </div>
    </div>
  );
}

function Section({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="mb-4">
      <div className="text-[10px] uppercase tracking-wider text-text-dimmer font-semibold mb-2">
        {label}
      </div>
      {children}
    </div>
  );
}

function KV({ k, v }: { k: string; v: string }) {
  return (
    <div className="flex justify-between py-0.5 text-[11px]">
      <span className="text-text-dim">{k}</span>
      <span className="text-text font-medium">{v}</span>
    </div>
  );
}
