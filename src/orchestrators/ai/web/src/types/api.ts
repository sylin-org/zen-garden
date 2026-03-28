/** Mirrors domain::types::OfferingKind */
export type OfferingKind =
  | 'ollama' | 'comfyui' | 'whispercpp' | 'speaches'
  | 'openedai-speech' | 'infinity' | 'libretranslate'
  | 'huggingface' | 'openai' | 'anthropic' | 'stability-ai'
  | 'elevenlabs' | 'cohere' | 'deepgram' | 'google'

/** Mirrors domain::types::Capability */
export type Capability =
  | 'generate' | 'chat' | 'embed' | 'vision' | 'tools' | 'think'
  | 'imagine' | 'edit' | 'render' | 'transcribe' | 'speak'
  | 'rerank' | 'translate'

/** Mirrors domain::types::Verdict */
export type Verdict = 'fast' | 'degraded' | 'vetoed' | 'blocked'

/** Mirrors domain::types::InstanceHealth — tagged enum via #[serde(tag = "status")] */
export type InstanceHealth =
  | { status: 'profiling' }
  | { status: 'healthy' }
  | { status: 'unhealthy'; reason: string }

export interface Stone {
  id: string
  name: string
}

export interface Gpu {
  name: string | null
  compute: 'gpu' | 'cpu'
}

export interface Vram {
  total_bytes: number
  budget_bytes: number
  free_bytes: number | null
}

export interface LoadedModel {
  name: string
  vram_bytes: number
  expires_at: string | null
}

export interface ServiceInstance {
  stone: Stone
  endpoint: string
  kind: OfferingKind
  gpu: Gpu
  vram: Vram
  health: InstanceHealth
  models_available: string[]
  models_loaded: LoadedModel[]
  capabilities: Capability[]
  queue_depth: number
  priority: number
  metadata: unknown
}

export interface ModelInfo {
  name: string
  parameter_count: number | null
  parameter_size: string | null
  quantization_level: string | null
  family: string | null
  capabilities: string[]
  format: string | null
  size_disk: number
  vram_bytes: number | null
  context_length: number | null
}

export interface Tier {
  label: string
  vram_bytes: number
  endpoints: string[]
}

export interface StoneVramBudget {
  stone: Stone
  total_bytes: number
  used_bytes: number
  free_bytes: number
  per_offering: Array<{ kind: OfferingKind; used_bytes: number; model_count: number }>
}

export interface MetricsSnapshot {
  requests_total: number
  tokens_in_total: number
  tokens_out_total: number
  errors_total: number
  per_stone: Record<string, {
    requests: number
    tokens_in: number
    tokens_out: number
    errors: number
    total_duration_ns: number
    eval_duration_ns: number
  }>
  per_model: Record<string, number>
  started_at: string | null
  snapshot_at: string | null
}

export interface PlacementPlan {
  assignments: Record<string, string[]>
  computed_at: string | null
  stable: boolean
}

export interface GpuMatrixEntry {
  model: string
  capability: Capability
  stone_name: string
  endpoint: string
  gpu_model: string
  verdict: Verdict
  median_tps: number
  cold_start_ms: number
  valid_ratio: number | null
}

export interface BenchmarkData {
  id: string
  status: 'idle' | 'running' | 'completed' | 'cancelled' | 'failed'
  started_at: string | null
  completed_at: string | null
  stones: unknown[]
  gpu_matrix: { generated_at: string | null; entries: GpuMatrixEntry[] }
  error: string | null
}

export interface OrchestratorJob {
  id: string
  kind: string
  status: 'queued' | 'running' | 'completed' | 'failed'
  detail: string
  started_at: string
  completed_at: string | null
  error: string | null
}

export interface OrchestratorConfig {
  auto_pull_mode: 'off' | 'sync' | 'on_demand'
  delete_on_idle: boolean
  metrics_enabled: boolean
  pins: Record<string, string>
}

/** The full snapshot from /api/status (and status.snapshot SSE events) */
export interface Snapshot {
  orchestrator: {
    version: string
    uptime_secs: number
    offerings_registered: number
    instances_discovered: number
    models_known: number
  }
  instances: ServiceInstance[]
  models: ModelInfo[]
  offering_counts: Record<string, number>
  tiers: Tier[]
  vram_budgets: StoneVramBudget[]
  metrics: MetricsSnapshot
  demand_shares: Record<string, number>
  placement: PlacementPlan
  benchmark: BenchmarkData
  recommended_models: Record<string, string>
  config: OrchestratorConfig
  jobs: OrchestratorJob[]
}
