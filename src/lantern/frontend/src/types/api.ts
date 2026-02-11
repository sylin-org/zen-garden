/** Stone as returned by GET /api/v1/garden/stones */
export interface Stone {
  stone_id: string;
  stone_name: string;
  endpoint: string;
  moss_version: string;
  health: string;
  status: string;
  discovered_at: string;
  last_seen: string;
  tags: string[];
  /** Deterministic HSL color for visual identity */
  color: string;
  services: ServiceView[];
  resources: Resources | null;
  capabilities: Capabilities | null;
  offerings: Offering[];
  seed_banks: SeedBank[];
  companions: Companion[];
}

export interface ServiceView {
  offering_id: string;
  name: string;
  offering: string;
  category: string;
  status: string;
}

export interface Resources {
  cpu_cores: number;
  cpu_percent: number;
  memory_total_bytes: number;
  memory_used_bytes: number;
  memory_percent: number;
  disk_total_gb: number;
  disk_used_gb: number;
  disk_percent: number;
  uptime_seconds: number;
}

export interface Capabilities {
  hardware?: {
    cpu: { cores: number; model?: string; arch?: string };
    memory: { total_mb: number };
  };
  runtime?: {
    os: string;
    hostname?: string;
  };
}

export interface Offering {
  offering_id: string;
  name: string;
  offering: string;
  category: string;
  status: string;
  health: string;
  port: number;
  instance_name: string | null;
}

export interface SeedBank {
  id: string;
  name: string;
  capacity_bytes: number;
  used_bytes: number;
  visibility: string;
  online: boolean;
  pool_id: string | null;
}

export interface Companion {
  id: string;
  name: string;
  status: string;
  port: number;
}

/** Offering group from GET /api/v1/garden/offerings */
export interface OfferingGroup {
  offering: string;
  category: string;
  instances: OfferingInstance[];
}

export interface OfferingInstance {
  stone_id: string;
  stone_name: string;
  offering_id: string;
  name: string;
  status: string;
  health: string;
  port: number;
}

/** Pond member from GET /api/v1/garden/pond */
export interface PondMember {
  stone_id: string;
  stone_name: string;
  endpoint: string;
  health: string;
  status: string;
  mac?: string;
  tags: string[];
  services_count: number;
  os?: string;
  cpu_cores?: number;
  memory_mb?: number;
}

/** Seed bank view from GET /api/v1/garden/seeds */
export interface SeedBankView {
  stone_id: string;
  stone_name: string;
  id: string;
  name: string;
  capacity_bytes: number;
  used_bytes: number;
  visibility: string;
  online: boolean;
  pool_id: string | null;
}

/** Activity event from GET /api/v1/garden/activity */
export interface ActivityEvent {
  timestamp: string;
  event_type: string;
  message: string;
  stone_name?: string;
  data?: unknown;
}

/** Health endpoint response */
export interface HealthResponse {
  status: string;
  lantern_name: string;
  port: number;
  stones_online: number;
  stones_total: number;
  uptime_seconds: number;
}
