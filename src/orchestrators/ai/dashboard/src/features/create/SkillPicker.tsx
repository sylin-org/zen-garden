import { Link } from "react-router-dom";
import type { SkillListEntry } from "../../api/types";

interface Props {
  skills: SkillListEntry[];
  modality: string;
  leaf: string;
}

export default function SkillPicker({ skills, modality, leaf }: Props) {
  return (
    <div className="p-6">
      <h3 className="text-sm font-semibold text-text-dim mb-3">
        Pick a style ({skills.length} available)
      </h3>
      <div className="grid grid-cols-2 gap-3">
        {skills.map((s) => (
          <Link
            key={s.id}
            to={`/create/${modality}/${leaf}/${s.id}`}
            className="p-4 rounded-lg bg-surface border border-border hover:border-accent
                       transition-colors group"
          >
            <div className="text-[13px] font-medium group-hover:text-accent transition-colors truncate">
              {s.display.name}
            </div>
            {s.display.description && (
              <div className="text-[11px] text-text-dim mt-1 line-clamp-2">
                {s.display.description}
              </div>
            )}
            <div className="text-[10px] text-text-dimmer mt-2">{s.provider}</div>
          </Link>
        ))}
      </div>
    </div>
  );
}
