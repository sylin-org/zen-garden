// ── API Types ───────────────────────────────────────────────────
// Mirrors the Rust structs in src/api/dashboard.rs exactly.

export type CapabilityState = "active" | "needs_setup" | "not_installed" | "degraded";

export interface CapabilityStatus {
  capability: string;
  state: CapabilityState;
  recommended_model: string | null;
  model_count: number;
  offering_count: number;
  offerings: string[];
  instance_count: number;
  healthy_instance_count: number;
}

export interface StoneOfferingStatus {
  kind: string;
  model_count: number;
  loaded_count: number;
  healthy: boolean;
}

export interface StoneStatus {
  id: string;
  name: string;
  gpu: string | null;
  vram_total_mb: number;
  vram_used_mb: number;
  offerings: StoneOfferingStatus[];
  health: string;
}

export interface InstanceStatus {
  endpoint: string;
  stone_name: string;
  kind: string;
  health: string;
  models_available: string[];
  models_loaded: string[];
  vram_total_mb: number;
  vram_budget_mb: number;
  gpu: string | null;
  queue_depth: number;
  capabilities: string[];
  priority: number;
}

export interface ModelPlacement {
  stone: string;
  endpoint: string;
  offering: string;
  loaded: boolean;
}

export interface ModelMetadata {
  parameter_count: number | null;
  parameter_size: string | null;
  quantization_level: string | null;
  family: string | null;
  families: string[];
  format: string | null;
  size_disk: number;
  vram_bytes: number | null;
  context_length: number | null;
}

export interface ModelStatus {
  model: string;
  parameters: string | null;
  model_identity: string;
  capabilities: string[];
  specializations: string[];
  metadata: ModelMetadata;
  instances: string[];
  instance_count: number;
  available_on: ModelPlacement[];
}

export interface FeatureConfig {
  auto_pull_mode: "off" | "sync" | "on_demand";
  delete_on_idle: boolean;
  metrics_enabled: boolean;
  pins: Record<string, string>;
}

export interface StoneConfig {
  vram_budget_mb: number | null;
}

export interface InferenceDefaults {
  temperature?: number | null;
  max_tokens?: number | null;
  top_p?: number | null;
}

export interface OrchestratorConfig {
  features: FeatureConfig;
  stones: Record<string, StoneConfig>;
  proxies: Record<string, boolean>;
  defaults: Record<string, InferenceDefaults>;
}

export interface OrchestratorJob {
  id: string;
  kind: Record<string, unknown>;
  status: "Queued" | "Running" | "Completed" | "Failed";
  progress: string | null;
  started_at: string;
  completed_at: string | null;
  error: string | null;
}

export interface ConfiguredProvider {
  name: string;
  kind: string;
  base_url: string;
  masked_key: string;
  enabled: boolean;
  priority: number;
  capabilities: string[];
  model_count: number;
}

export interface DashboardStatus {
  capabilities: CapabilityStatus[];
  stones: StoneStatus[];
  instances: InstanceStatus[];
  models: ModelStatus[];
  config: OrchestratorConfig;
  jobs: OrchestratorJob[];
  recommendations: Record<string, string>;
  uptime_secs: number;
  version: string;
}

// ── Display Helpers ─────────────────────────────────────────────

/** User-facing capabilities in display order.
 * Names describe the output type (Image, Video, Speech, Music)
 * or the action when output is text (Chat, Transcribe, Translate). */
export const ALL_CAPABILITIES = [
  "chat",
  "think",
  "tools",
  "translate",
  "vision",
  "ocr",
  "transcribe",
  "embed",
  "rerank",
  "image",
  "video",
  "speech",
  "music",
] as const;

/** Human-friendly labels for capabilities. */
export const CAPABILITY_LABELS: Record<string, string> = {
  chat: "Chat",
  think: "Think",
  tools: "Tools",
  translate: "Translate",
  vision: "Vision",
  ocr: "OCR",
  transcribe: "Transcribe",
  embed: "Embed",
  rerank: "Rerank",
  image: "Image",
  video: "Video",
  speech: "Speech",
  music: "Music",
};

// ── Skill Types ─────────────────────────────────────────────────

export interface SkillContentSlot {
  role: string;
  content_type: "image" | "text";
  required: boolean;
  /** If set, render as a paint overlay on the referenced role's image (e.g., "source"). */
  overlay?: string;
}

export interface SkillModelRef {
  filename: string;
  model_type: string;
  description: string | null;
}

export interface SkillInfo {
  name: string;
  display_name: string;
  capability: string;
  description: string;
  available: boolean;
  vram_mb: number;
  content_slots: SkillContentSlot[];
  diagram: string | null;
  required_models: SkillModelRef[];
  instances: SkillInstanceView[];
}

export interface SkillInstanceView {
  stone_name: string;
  endpoint: string;
  ready: boolean;
  reason: string;
  vram_mb: number;
}

// ── Mapping-driven skill form (replaces JSON Schema) ──────────

export interface SkillFormResponse {
  display_name: string;
  description: string;
  content_slots: SkillContentSlot[];
  mappings: SkillMapping[];
  diagram: string | null;
}

export type SkillMapping =
  | { type: "content"; role: string; content_type: "image" | "text"; placeholder: string }
  | { type: "param"; field: string; node: string; input: string; label: string; default?: unknown } & ParamTypeDef;

export type ParamTypeDef =
  | { param_type: "options"; options: ParamOption[] }
  | { param_type: "range"; min: number; max: number; step?: number }
  | { param_type: "auto"; kind: "random_int" }
  | { param_type: "text" };

export interface ParamOption {
  value: unknown;
  label?: string;
}

export function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(1024));
  const value = bytes / Math.pow(1024, i);
  return `${value.toFixed(value < 10 ? 2 : 1)} ${units[i]}`;
}

export function formatUptime(secs: number): string {
  const d = Math.floor(secs / 86400);
  const h = Math.floor((secs % 86400) / 3600);
  const m = Math.floor((secs % 3600) / 60);
  if (d > 0) return `${d}d ${h}h`;
  if (h > 0) return `${h}h ${m}m`;
  return `${m}m`;
}
