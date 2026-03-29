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

export interface ModelStatus {
  name: string;
  capabilities: string[];
  parameter_size: string | null;
  quantization_level: string | null;
  family: string | null;
  size_disk: number;
  vram_bytes: number | null;
  context_length: number | null;
  available_on: ModelPlacement[];
}

export interface FeatureConfig {
  auto_pull_mode: "Off" | "Sync" | "OnDemand";
  delete_on_idle: boolean;
  metrics_enabled: boolean;
  pins: Record<string, string>;
}

export interface StoneConfig {
  vram_budget_mb: number | null;
}

export interface OrchestratorConfig {
  features: FeatureConfig;
  stones: Record<string, StoneConfig>;
  proxies: Record<string, boolean>;
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

/** All 13 capabilities in display order. */
export const ALL_CAPABILITIES = [
  "generate",
  "chat",
  "embed",
  "vision",
  "tools",
  "think",
  "imagine",
  "edit",
  "render",
  "transcribe",
  "speak",
  "rerank",
  "translate",
] as const;

/** Human-friendly labels for capabilities. */
export const CAPABILITY_LABELS: Record<string, string> = {
  generate: "Generate",
  chat: "Chat",
  embed: "Embed",
  vision: "Vision",
  tools: "Tools",
  think: "Think",
  imagine: "Imagine",
  edit: "Edit",
  render: "Render",
  transcribe: "Transcribe",
  speak: "Speak",
  rerank: "Rerank",
  translate: "Translate",
};

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
