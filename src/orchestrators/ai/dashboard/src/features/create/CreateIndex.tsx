import { useCatalog } from "../../contexts/CatalogContext";
import { Link } from "react-router-dom";

export default function CreateIndex() {
  const { catalog, loading } = useCatalog();

  if (loading || !catalog) {
    return (
      <div className="flex items-center justify-center h-full text-text-dim text-sm">
        Loading catalog...
      </div>
    );
  }

  return (
    <div className="flex-1 p-6 overflow-y-auto">
      {catalog.modalities.map((mod) => {
        const prims = catalog.primitives.filter((p) => p.modality === mod.id);
        if (prims.length === 0) return null;

        return (
          <div key={mod.id} className="mb-8">
            {/* Modality header */}
            <h2 className="text-sm font-semibold text-text-dim mb-3 flex items-center gap-2">
              <span className="text-lg">{mod.icon}</span>
              {mod.label}
            </h2>

            {/* Primitives */}
            {prims.map((prim) => {
              const [, leaf] = prim.action.split(".");
              const skills = catalog.skills.filter((s) => s.primitive === prim.action);

              return (
                <div key={prim.action} className="mb-4">
                  <Link
                    to={`/create/${mod.id}/${leaf}`}
                    className="block p-4 rounded-lg bg-surface border border-border
                               hover:border-accent transition-colors mb-2"
                  >
                    <div className="flex items-center justify-between">
                      <div className="text-[13px] font-medium">{capitalize(leaf)}</div>
                      <div className="flex items-center gap-2">
                        {skills.length > 0 && (
                          <span className="text-[10px] bg-surface-3 text-text-dimmer px-1.5 py-px rounded">
                            {skills.length} {skills.length === 1 ? "style" : "styles"}
                          </span>
                        )}
                        <span className="text-[10px] bg-surface-3 text-text-dimmer px-1.5 py-px rounded">
                          {prim.providers.length} {prim.providers.length === 1 ? "provider" : "providers"}
                        </span>
                      </div>
                    </div>
                    <div className="text-[11px] text-text-dim mt-1">{prim.summary}</div>
                  </Link>

                  {/* Skills grid */}
                  {skills.length > 0 && (
                    <div className="grid grid-cols-3 gap-2 pl-4">
                      {skills.map((skill) => (
                        <Link
                          key={skill.id}
                          to={`/create/${mod.id}/${leaf}/${skill.id}`}
                          className="p-3 rounded-lg bg-surface-2 border border-border
                                     hover:border-accent transition-colors group"
                        >
                          {skill.display.preview_image && (
                            <img
                              src={skill.display.preview_image}
                              alt={skill.display.name}
                              className="w-full h-20 object-cover rounded mb-2"
                            />
                          )}
                          <div className="text-[11px] font-medium group-hover:text-accent transition-colors truncate">
                            {skill.display.name}
                          </div>
                          {skill.display.description && (
                            <div className="text-[10px] text-text-dimmer mt-0.5 line-clamp-2">
                              {skill.display.description}
                            </div>
                          )}
                          <div className="text-[9px] text-text-dimmer mt-1">{skill.provider}</div>
                        </Link>
                      ))}
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        );
      })}
    </div>
  );
}

function capitalize(s: string): string {
  return s.charAt(0).toUpperCase() + s.slice(1);
}
