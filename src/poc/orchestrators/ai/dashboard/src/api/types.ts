// TypeScript types derived from live API responses (2026-04-10).
// These mirror the Rust structs exactly — do not invent fields.

// ── Catalog ──────────────────────────────────────────────────

export interface CatalogSummary {
  modalities: Modality[];
  primitives: CatalogPrimitive[];
  skills: CatalogSkill[];
  providers: CatalogProvider[];
}

export interface Modality {
  id: string;
  label: string;
  icon: string;
}

export interface CatalogPrimitive {
  action: string;
  modality: string;
  summary: string;
  providers: CatalogPrimitiveProvider[];
  vocabulary?: unknown;
}

export interface CatalogPrimitiveProvider {
  name: string;
  media_inputs: MediaInput[];
}

export interface MediaInput {
  field: string;
  delivery: "base64" | "by_id" | "transfer";
  accepted_types: string[];
  overlay?: string | null;
}

export interface CatalogSkill {
  action: string;
  primitive: string;
  id: string;
  display: SkillDisplay;
  provider: string;
  parameters: CatalogFieldParam[];
}

export interface SkillDisplay {
  name: string;
  description?: string | null;
  tags: string[];
  preview_image?: string | null;
}

export interface CatalogFieldParam {
  field: string;
  required: boolean;
  pinnable: boolean;
  label?: string | null;
  default?: unknown;
  auto?: AutoDescriptor | null;
  type?: unknown;
}

export interface CatalogProvider {
  name: string;
  enabled: boolean;
  version: number;
  capability_count: number;
  skill_count: number;
}

// ── Workspace (from GET /v1/{mod}/{leaf}[/{skill}]) ──────────

/** The full workspace definition returned by the introspect endpoint. */
export interface WorkspaceSpec {
  kind: "primitive" | "skill";
  primitive: string;
  skill_id?: string;
  display: { name: string; description?: string; tags?: string[]; preview_image?: string };
  routing: { providers: string[]; will_run_on?: string; status: string };
  invocation: { method: string; url: string; content_type: string };
  /** Pre-assembled dispatch body with defaults. POST-able as-is. */
  payload: Record<string, unknown>;
  /** Dotted-path → widget descriptor. Rendering instructions only. */
  fields: Record<string, FieldDescriptor>;
  examples?: WorkspaceExample[];
  skills_available?: SkillListEntry[];
  media_inputs?: MediaInput[];
}

export interface FieldDescriptor {
  label: string;
  type: string;
  widget: string;
  required?: boolean;
  description?: string;
  placeholder?: string;
  min?: number;
  max?: number;
  step?: number;
  options?: unknown[];
  auto?: AutoDescriptor;
}

export interface AutoDescriptor {
  default: string;
  description?: string;
}

export interface WorkspaceExample {
  label: string;
  description?: string;
  payload: Record<string, unknown>;
}

export interface SkillListEntry {
  id: string;
  provider: string;
  display: { name: string; description?: string; tags?: string[] };
  url: string;
}

/** Legacy field shape used by widget components. Bridged from
 * FieldDescriptor via fieldDescToLegacy(). TODO: update widgets
 * to use FieldDescriptor directly. */
export interface CatalogField {
  field: string;
  required: boolean;
  pinnable: boolean;
  label?: string;
  description?: string;
  field_type?: string;
  widget?: string;
  default?: unknown;
  min?: number;
  max?: number;
  step?: number;
  options?: unknown[];
  placeholder?: string;
  auto?: AutoDescriptor;
}

// ── Dispatch ─────────────────────────────────────────────────

export interface DispatchResponse {
  output?: Record<string, unknown>;
  error?: ErrorBody;
  _meta: Meta;
}

export interface ErrorBody {
  code: string;
  message: string;
  details?: unknown;
}

export interface Meta {
  correlation_id: string;
  request_id: string;
  response_id?: string;
  action: string;
  provider?: string;
  model?: string;
  mode: "sync" | "async" | "stream";
  received_at: string;
  completed_at?: string;
}

// ── Jobs ─────────────────────────────────────────────────────

export type JobState = "queued" | "running" | "done" | "failed" | "cancelled";
export type JobCategory = "api" | "provider" | "background";

export interface JobView {
  id: string;
  correlation_id: string;
  category: JobCategory;
  owner?: string;
  action?: string;
  state: JobState;
  progress?: JobProgress;
  eta_seconds?: number;
  created_at: string;
  updated_at: string;
  terminal_at?: string;
  metadata?: unknown;
}

export interface JobProgress {
  current: number;
  total?: number;
  label?: string;
}

export interface JobListResponse {
  jobs: JobView[];
}

// ── Skills ───────────────────────────────────────────────────

export interface SkillListResponse {
  version: number;
  count: number;
  skills: SkillView[];
}

export interface SkillView {
  id: string;
  provider: string;
  primitive: string;
  display: SkillDisplay;
  parameters: CatalogFieldParam[];
}

export interface ImportResponse {
  moniker: string;
  primitive: string;
  draft_dir: string;
  links: { self: string; events: string };
}

// ── Media ────────────────────────────────────────────────────

export interface MediaEntry {
  media_id: string;
  content_hash: string;
  content_type: string;
  size_bytes: number;
  metadata?: unknown;
  source: { kind: string; provider?: string; action?: string };
  lifecycle: { state: string; expires_at?: string };
  created_at: string;
}

export interface MediaListResponse {
  media: MediaEntry[];
}

// ── Resources ────────────────────────────────────────────────

export interface ResourcesResponse {
  count: number;
  stones: StoneResources[];
}

export interface StoneResources {
  name: string;
  gpus: GpuDevice[];
  memory: { total_mb?: number; committed_mb: number };
  claims: Record<string, unknown>;
}

export interface GpuDevice {
  index: number;
  name: string;
  vendor: string;
  compute_stack: string[];
  total_vram_mb?: number;
  headroom_mb: number;
  committed_mb: number;
  mode: string;
}

// ── Health ────────────────────────────────────────────────────

export interface HealthResponse {
  status: string;
  directory_version: number;
  providers_registered: number;
  providers_enabled: number;
}

// ── Introspection ────────────────────────────────────────────

export interface IntrospectionResponse {
  kind: "primitive" | "skill";
  primitive: string;
  skill_id?: string;
  display: { name: string; description?: string; tags?: string[] };
  routing: {
    providers: string[];
    will_run_on?: string;
    status: string;
  };
  invocation: {
    method: string;
    url: string;
    content_type: string;
  };
  parameters?: unknown[];
  example?: { url: string; body: unknown };
}

// ── Requests (ORCH-0033) ─────────────────────────────────────

export type RequestStatus = "running" | "success" | "failure";

export interface PersistedRequest {
  id: string;
  correlation_id: string;
  created_at: string;
  completed_at?: string;
  parent_id?: string;
  action: string;
  status: RequestStatus;
  input: unknown;
  selectors: { provider?: string; model?: string; variant?: string };
  output?: unknown;
  error?: { code: string; message: string; details?: unknown };
  media_inputs: RequestMediaRef[];
  media_outputs: RequestMediaRef[];
  meta: {
    provider?: string;
    model?: string;
    stone?: string;
    latency_ms?: number;
    tokens_in?: number;
    tokens_out?: number;
    summary?: string;
  };
  pinned: boolean;
  job_id?: string;
}

export interface RequestMediaRef {
  media_id: string;
  field: string;
  content_type: string;
}

export interface RequestListResponse {
  count: number;
  requests: PersistedRequest[];
}

// ── SSE Events ───────────────────────────────────────────────

export interface SSEEvent {
  seq: number;
  topic: string;
  at: string;
  payload: unknown;
}
