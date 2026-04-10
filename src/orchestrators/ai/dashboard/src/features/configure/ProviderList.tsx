import { useState } from "react";
import { useCatalog } from "../../contexts/CatalogContext";
import type { CatalogProvider } from "../../api/types";

export default function ProviderList() {
  const { catalog } = useCatalog();
  const [selected, setSelected] = useState<CatalogProvider | null>(null);

  if (!catalog) {
    return <div className="p-4 text-text-dimmer text-sm">Loading...</div>;
  }

  return (
    <div className="flex h-full">
      {/* Master */}
      <div className="flex-1 overflow-y-auto border-r border-border">
        {catalog.providers.map((p) => (
          <div
            key={p.name}
            onClick={() => setSelected(p)}
            className={[
              "flex items-center gap-3 px-4 py-3 border-b border-border cursor-pointer transition-colors",
              selected?.name === p.name ? "bg-accent-bg" : "hover:bg-surface",
            ].join(" ")}
          >
            <div className={`w-2 h-2 rounded-full shrink-0 ${p.enabled ? "bg-green" : "bg-red"}`} />
            <div className="flex-1 min-w-0">
              <div className="text-[12px] font-medium">{p.name}</div>
              <div className="text-[10px] text-text-dimmer">
                {p.capability_count} capabilities · {p.skill_count} skills
              </div>
            </div>
            <div className="text-[10px] text-text-dimmer">v{p.version}</div>
          </div>
        ))}
      </div>

      {/* Detail */}
      <div className="w-[350px] shrink-0 overflow-y-auto bg-surface">
        {selected ? (
          <ProviderDetail provider={selected} />
        ) : (
          <div className="flex items-center justify-center h-full text-text-dimmer text-xs italic">
            Select a provider
          </div>
        )}
      </div>
    </div>
  );
}

function ProviderDetail({ provider }: { provider: CatalogProvider }) {
  const { catalog } = useCatalog();
  const primitives = catalog?.primitives.filter((p) =>
    p.providers.some((pr) => pr.name === provider.name),
  ) ?? [];
  const skills = catalog?.skills.filter((s) => s.provider === provider.name) ?? [];

  return (
    <div className="p-5">
      <div className="flex items-center gap-2 mb-4">
        <div className={`w-2.5 h-2.5 rounded-full ${provider.enabled ? "bg-green" : "bg-red"}`} />
        <h3 className="text-sm font-semibold">{provider.name}</h3>
      </div>

      <Section label="Status">
        <KV k="Enabled" v={provider.enabled ? "Yes" : "No"} />
        <KV k="Version" v={String(provider.version)} />
        <KV k="Capabilities" v={String(provider.capability_count)} />
        <KV k="Skills" v={String(provider.skill_count)} />
      </Section>

      {primitives.length > 0 && (
        <Section label="Primitives">
          {primitives.map((p) => (
            <div key={p.action} className="text-[11px] text-text-dim py-0.5">
              {p.action}
            </div>
          ))}
        </Section>
      )}

      {skills.length > 0 && (
        <Section label="Skills">
          {skills.map((s) => (
            <div key={s.id} className="py-1">
              <div className="text-[11px] text-text">{s.display.name}</div>
              <div className="text-[10px] text-text-dimmer">{s.id}</div>
            </div>
          ))}
        </Section>
      )}
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
