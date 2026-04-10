import { Link, useParams, useLocation } from "react-router-dom";
import { useCatalog } from "../contexts/CatalogContext";

/**
 * Breadcrumb header with sibling navigation (ORCH-0032).
 *
 * At the primitive level, sibling primitives within the same modality
 * are shown inline as clickable links next to the active segment.
 *
 * At the skill level, the skill display name is shown as the final
 * segment (no sibling skills — the picker handles that).
 */
export default function Breadcrumb() {
  const location = useLocation();
  const params = useParams();
  const { catalog } = useCatalog();

  const segments = location.pathname.split("/").filter(Boolean);
  // segments[0] = "create" | "manage" | "configure"
  // segments[1] = modality or section (e.g. "text", "skills")
  // segments[2] = leaf (e.g. "chat", "generate")
  // segments[3] = skill id

  if (segments.length === 0) return null;

  const group = segments[0];
  const groupLabel = group.charAt(0).toUpperCase() + group.slice(1);

  // Simple breadcrumb for manage/configure
  if (group !== "create") {
    return (
      <Header>
        <BreadcrumbLink to={`/${group}`} label={groupLabel} />
        {segments[1] && (
          <>
            <Sep />
            <ActiveSegment label={capitalize(segments[1])} />
          </>
        )}
      </Header>
    );
  }

  // Create breadcrumb with sibling nav
  const modality = params.modality;
  const leaf = params.leaf;
  const skill = params.skill;

  if (!modality) {
    // /create directory
    return (
      <Header>
        <ActiveSegment label={groupLabel} />
      </Header>
    );
  }

  // Find modality info from catalog
  const modalityInfo = catalog?.modalities.find((m) => m.id === modality);
  const modalityLabel = modalityInfo?.label ?? capitalize(modality);

  if (!leaf) {
    return (
      <Header>
        <BreadcrumbLink to="/create" label={groupLabel} />
        <Sep />
        <ActiveSegment label={modalityLabel} />
      </Header>
    );
  }

  // Find sibling primitives in this modality
  const siblings = catalog?.primitives.filter((p) => p.modality === modality) ?? [];

  if (skill) {
    // Skill level — show full path, no sibling nav
    return (
      <Header>
        <BreadcrumbLink to="/create" label={groupLabel} />
        <Sep />
        <BreadcrumbLink to={defaultRoute(modality)} label={modalityLabel} />
        <Sep />
        <BreadcrumbLink to={`/create/${modality}/${leaf}`} label={capitalize(leaf)} />
        <Sep />
        <ActiveSegment label={skill} />
      </Header>
    );
  }

  // Primitive level — show siblings
  return (
    <Header>
      <BreadcrumbLink to="/create" label={groupLabel} />
      <Sep />
      <BreadcrumbLink to={defaultRoute(modality)} label={modalityLabel} />
      <Sep />
      <SiblingNav
        siblings={siblings.map((p) => {
          const [, l] = p.action.split(".");
          return { leaf: l, label: capitalize(l) };
        })}
        activeLeaf={leaf}
        modality={modality}
      />
    </Header>
  );
}

function Header({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex items-center gap-1 px-6 py-3 border-b border-border bg-surface text-[12px] shrink-0">
      {children}
    </div>
  );
}

function BreadcrumbLink({ to, label }: { to: string; label: string }) {
  return (
    <Link to={to} className="text-text-dimmer hover:text-text-dim transition-colors">
      {label}
    </Link>
  );
}

function Sep() {
  return <span className="text-text-dimmer mx-0.5">›</span>;
}

function ActiveSegment({ label }: { label: string }) {
  return <span className="text-text font-semibold">{label}</span>;
}

function SiblingNav({
  siblings,
  activeLeaf,
  modality,
}: {
  siblings: { leaf: string; label: string }[];
  activeLeaf: string;
  modality: string;
}) {
  return (
    <div className="flex items-center gap-3">
      {siblings.map((s) => {
        const isActive = s.leaf === activeLeaf;
        return isActive ? (
          <span key={s.leaf} className="text-text font-semibold">
            {s.label}
          </span>
        ) : (
          <Link
            key={s.leaf}
            to={`/create/${modality}/${s.leaf}`}
            className="text-text-dimmer hover:text-accent transition-colors"
          >
            {s.label}
          </Link>
        );
      })}
    </div>
  );
}

function capitalize(s: string): string {
  return s.charAt(0).toUpperCase() + s.slice(1);
}

const MODALITY_DEFAULTS: Record<string, string> = {
  text: "/create/text/chat",
  image: "/create/image/generate",
  audio: "/create/audio/generate",
};

function defaultRoute(modality: string): string {
  return MODALITY_DEFAULTS[modality] ?? `/create/${modality}`;
}
