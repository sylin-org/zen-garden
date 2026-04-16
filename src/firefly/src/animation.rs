//! Firefly animation engine
//!
//! Manages the ambient "firefly" LED animation that serves as the baseline
//! visual state. Events temporarily override this baseline, then it resumes.

use std::sync::Arc;
use std::time::{Duration, Instant};

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use tokio::sync::RwLock;

use crate::firefly::{Firefly, FireflyKind};

/// Default health label when none is available.
pub const DEFAULT_HEALTH_LABEL: &str = "thriving";

/// Matrix dimensions
const GRID_SIZE: u8 = 5;
const TOTAL_PIXELS: usize = 25;

/// Animation runs at this rate
const FRAME_RATE: u64 = 30; // fps
const FRAME_DURATION: Duration = Duration::from_millis(1000 / FRAME_RATE);

/// Warm white color (slightly amber, like real fireflies)
const WARM_WHITE: (u8, u8, u8) = (255, 180, 100);
/// Storage activity indicator (seed bank green)
const STORAGE_GREEN: (u8, u8, u8) = (80, 255, 80);
/// Service activity indicator
const SERVICE_BLUE: (u8, u8, u8) = (80, 140, 255);
/// Storage departure color (warm amber)
const STORAGE_AMBER: (u8, u8, u8) = (255, 160, 40);

/// Center pixel coordinate
const CENTER: (u8, u8) = (2, 2);

/// Edge pixels for swarm spawn points (all pixels on the border)
const EDGE_PIXELS: [(u8, u8); 16] = [
    (0, 0),
    (1, 0),
    (2, 0),
    (3, 0),
    (4, 0), // top edge
    (4, 1),
    (4, 2),
    (4, 3), // right edge (excluding corners)
    (4, 4),
    (3, 4),
    (2, 4),
    (1, 4),
    (0, 4), // bottom edge
    (0, 3),
    (0, 2),
    (0, 1), // left edge (excluding corners)
];

/// Sprite lifecycle phase
#[derive(Debug, Clone, Copy, PartialEq)]
enum Phase {
    FadeIn,
    Peak,
    FadeOut,
    /// Waiting to spawn (not visible)
    Dormant,
}

/// A single pixel-level animation sprite (the "firefly" of the matrix
/// animation, renamed from `Firefly` to avoid collision with the
/// domain entity in `crate::firefly`).
#[derive(Debug, Clone)]
struct Sprite {
    x: u8,
    y: u8,
    color: (u8, u8, u8),
    phase: Phase,
    /// Progress within current phase (0.0 to 1.0)
    progress: f32,
    /// When this firefly started its current phase
    phase_start: Instant,
}

impl Sprite {
    fn new_at(x: u8, y: u8, color: (u8, u8, u8)) -> Self {
        Self {
            x,
            y,
            color,
            phase: Phase::FadeIn,
            progress: 0.0,
            phase_start: Instant::now(),
        }
    }

    /// Calculate current brightness (0.0 to 1.0) based on phase and progress
    fn brightness(&self) -> f32 {
        match self.phase {
            Phase::FadeIn => ease_in_out(self.progress),
            Phase::Peak => 1.0,
            Phase::FadeOut => 1.0 - ease_in_out(self.progress),
            Phase::Dormant => 0.0,
        }
    }

    /// Get the current RGB values adjusted for brightness
    fn current_rgb(&self) -> (u8, u8, u8) {
        let b = self.brightness();
        (
            (self.color.0 as f32 * b) as u8,
            (self.color.1 as f32 * b) as u8,
            (self.color.2 as f32 * b) as u8,
        )
    }
}

/// Smooth easing function for natural fade
fn ease_in_out(t: f32) -> f32 {
    if t < 0.5 {
        2.0 * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
    }
}

/// Override types that temporarily replace baseline
#[derive(Debug, Clone)]
pub enum Override {
    /// Stone was tended - sparkle animation
    Tended,
    /// Health warning - amber pulse
    HealthWarning,
    /// Health error - red pulse
    HealthError,
    /// Service started - green bloom
    ServiceStarted,
    /// Service stopped - brief dim
    ServiceStopped,
    /// Storage detected - firefly swarm converging inward
    StorageDetected,
    /// Storage removed - firefly swarm dispersing outward
    StorageRemoved,
}

/// Health override display duration before yielding to baseline
const HEALTH_DISPLAY_SECS: u64 = 10;
/// Cooldown between health override cycles (baseline shows during this window)
const HEALTH_COOLDOWN_SECS: u64 = 5;

impl Override {
    /// How long this override lasts
    fn duration(&self) -> Duration {
        match self {
            Override::Tended => Duration::from_secs(3),
            Override::HealthWarning => Duration::from_secs(HEALTH_DISPLAY_SECS),
            Override::HealthError => Duration::from_secs(HEALTH_DISPLAY_SECS),
            Override::ServiceStarted => Duration::from_millis(1500),
            Override::ServiceStopped => Duration::from_millis(1000),
            Override::StorageDetected => Duration::from_millis(2000),
            Override::StorageRemoved => Duration::from_millis(1500),
        }
    }
}

/// Activity bonus per installed offering
const OFFERING_ACTIVITY_BONUS: f32 = 0.05;

/// Shared animation state updated by SSE events
pub struct Animation {
    /// System load (0.0 to 1.0) - base CPU/memory load
    pub load: f32,
    /// Number of installed offerings (adds activity bonus)
    pub offering_count: usize,
    /// Whether seed-bank (storage) is connected
    pub has_seed_bank: bool,
    /// Whether services are running (for blue fireflies)
    pub has_services: bool,
    /// Current health state
    pub health: Health,
    /// Raw health label (for OLED sync)
    pub health_label: String,
    /// Active override (if any)
    pub active_override: Option<(Override, Instant)>,
    /// Health cooldown: baseline shows until this instant, then health re-triggers
    health_cooldown_until: Option<Instant>,
    /// Whether animation should run
    pub enabled: bool,
    /// User-configured brightness (0-100), persisted
    pub brightness: u8,
    /// State directory for persistence
    state_dir: Option<std::path::PathBuf>,

    // Cached metrics (for OLED/T-Display updates and reconnect state)
    /// Stone name for display
    pub stone_name: Option<String>,
    /// Uptime in seconds
    pub uptime_seconds: u64,
    /// CPU percentage
    pub cpu_percent: u8,
    /// Memory percentage
    pub memory_percent: u8,
    /// Disk usage percentage
    pub disk_percent: u8,
    /// I/O activity percentage
    pub io_percent: u8,
    /// GPU utilization percentage
    pub gpu_percent: u8,
    /// Whether GPU is actively processing
    pub gpu_active: bool,
    /// Whether stone has a GPU
    pub has_gpu: bool,
    /// Whether stone is lantern
    pub is_lantern: bool,
    /// Whether cricket companion is present
    pub has_cricket: bool,
    /// Whether pond is active
    pub pond_active: bool,
    /// Current hour as decimal (e.g., 14.5 = 2:30 PM)
    pub hour: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Health {
    #[default]
    Thriving,
    Withering,
    Wilting,
}

/// Default brightness (fairly dim for ambient use)
const DEFAULT_BRIGHTNESS: u8 = 30;

impl Animation {
    pub fn new(state_dir: Option<std::path::PathBuf>) -> Self {
        // Load persisted brightness
        let brightness = state_dir
            .as_ref()
            .and_then(|dir| {
                let path = dir.join("brightness");
                std::fs::read_to_string(&path).ok()
            })
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(DEFAULT_BRIGHTNESS);

        Self {
            load: 0.0,
            offering_count: 0,
            has_seed_bank: false,
            has_services: false,
            health: Health::Thriving,
            health_label: DEFAULT_HEALTH_LABEL.to_string(),
            active_override: None,
            health_cooldown_until: None,
            enabled: true,
            brightness,
            state_dir,
            // Cached metrics
            stone_name: None,
            uptime_seconds: 0,
            cpu_percent: 0,
            memory_percent: 0,
            disk_percent: 0,
            io_percent: 0,
            gpu_percent: 0,
            gpu_active: false,
            has_gpu: false,
            is_lantern: false,
            has_cricket: false,
            pond_active: false,
            hour: 0.0,
        }
    }

    /// Set brightness and persist
    pub fn set_brightness(&mut self, brightness: u8) {
        self.brightness = brightness.min(100);

        // Persist to file
        if let Some(ref dir) = self.state_dir {
            if let Err(e) = std::fs::create_dir_all(dir) {
                tracing::warn!(error = %e, "Failed to create state directory");
                return;
            }
            let path = dir.join("brightness");
            if let Err(e) = std::fs::write(&path, self.brightness.to_string()) {
                tracing::warn!(error = %e, "Failed to persist brightness");
            }
        }
    }

    /// Calculate effective activity level (load + offering bonus)
    /// More offerings = more lively fireflies
    pub fn effective_activity(&self) -> f32 {
        let offering_bonus = self.offering_count as f32 * OFFERING_ACTIVITY_BONUS;
        (self.load + offering_bonus).clamp(0.0, 1.0)
    }

    /// Trigger an override animation
    pub fn trigger_override(&mut self, override_type: Override) {
        // Starting a health override clears any pending cooldown
        if matches!(override_type, Override::HealthWarning | Override::HealthError) {
            self.health_cooldown_until = None;
        }
        self.active_override = Some((override_type, Instant::now()));
    }

    /// Clear override (e.g., health recovered)
    pub fn clear_override(&mut self) {
        self.active_override = None;
        self.health_cooldown_until = None;
    }

    /// Check if override has expired; health overrides enter cooldown instead of clearing forever
    pub fn update_override(&mut self) {
        if let Some((ref override_type, start)) = self.active_override
            && start.elapsed() >= override_type.duration()
        {
            match override_type {
                Override::HealthWarning | Override::HealthError => {
                    // Enter cooldown — baseline shows, then health re-triggers
                    self.active_override = None;
                    self.health_cooldown_until =
                        Some(Instant::now() + Duration::from_secs(HEALTH_COOLDOWN_SECS));
                }
                _ => {
                    self.active_override = None;
                }
            }
        }
    }

    /// Re-trigger health override after cooldown expires (if health is still bad)
    pub fn maybe_retrigger_health(&mut self) {
        if self.active_override.is_some() {
            return;
        }
        if let Some(until) = self.health_cooldown_until
            && Instant::now() >= until
        {
            self.health_cooldown_until = None;
            match self.health {
                Health::Withering => self.trigger_override(Override::HealthWarning),
                Health::Wilting => self.trigger_override(Override::HealthError),
                Health::Thriving => {} // recovered during cooldown — no re-trigger
            }
        }
    }
}

/// Timing parameters derived from load
struct Tempo {
    fade_in: Duration,
    peak: Duration,
    fade_out: Duration,
    spawn_interval: Duration,
    max_concurrent: usize,
}

impl Tempo {
    /// Calculate tempo from load (0.0 = idle, 1.0 = busy)
    fn from_load(load: f32) -> Self {
        let load = load.clamp(0.0, 1.0);

        // Idle: slow, meditative rhythm
        // Busy: quick, energetic rhythm
        Self {
            fade_in: Duration::from_secs_f32(5.0 - load * 4.0), // 5s → 1s
            peak: Duration::from_secs_f32(1.0 - load * 0.7),    // 1s → 0.3s
            fade_out: Duration::from_secs_f32(5.0 - load * 4.0), // 5s → 1s
            spawn_interval: Duration::from_secs_f32(3.0 - load * 2.8), // 3s → 0.2s
            max_concurrent: 1 + (load * 3.0) as usize,          // 1 → 4
        }
    }
}

/// The animation engine
pub struct AnimationEngine {
    firefly: Arc<Firefly>,
    context: Arc<RwLock<Animation>>,
    sprites: Vec<Sprite>,
    last_spawn: Instant,
    /// Track which pixels are occupied (for spawning logic)
    occupied: [bool; TOTAL_PIXELS],
    /// Track which pixels were lit last frame (for clearing)
    prev_lit: [bool; TOTAL_PIXELS],
    /// Track if we were in override last frame (for clean transition)
    was_in_override: bool,
    /// Current frame for swarm animations (reset when override starts)
    swarm_frame: usize,
    rng: StdRng,
}

impl AnimationEngine {
    pub fn new(firefly: Arc<Firefly>, context: Arc<RwLock<Animation>>) -> Self {
        Self {
            firefly,
            context,
            sprites: Vec::new(),
            last_spawn: Instant::now(),
            occupied: [false; TOTAL_PIXELS],
            prev_lit: [false; TOTAL_PIXELS],
            was_in_override: false,
            swarm_frame: 0,
            rng: StdRng::from_os_rng(),
        }
    }

    /// Run the animation loop
    pub async fn run(mut self) {
        tracing::info!("Firefly animation engine started");

        let mut interval = tokio::time::interval(FRAME_DURATION);

        loop {
            interval.tick().await;

            let ctx = self.context.read().await;

            // Check if disabled - clear and wait
            if !ctx.enabled {
                drop(ctx);
                self.clear_all().await;
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }

            // Check for override
            let has_override = ctx.active_override.is_some();
            let override_type = ctx.active_override.as_ref().map(|(t, _)| t.clone());
            drop(ctx);

            if has_override {
                // Reset swarm frame when first entering override
                if !self.was_in_override {
                    self.swarm_frame = 0;
                }

                // Handle override animation (one frame at a time)
                if let Some(ref ov) = override_type {
                    self.render_override_frame(ov).await;
                }

                // Advance swarm frame
                self.swarm_frame += 1;

                // Update/expire override (health overrides enter cooldown)
                self.context.write().await.update_override();
                self.was_in_override = true;
            } else {
                // Transitioning from override back to baseline - clear for fresh start
                if self.was_in_override {
                    self.clear_all().await;
                    self.reset_fireflies();
                    self.was_in_override = false;
                    self.swarm_frame = 0;
                    tracing::debug!("Override ended, cleared display for baseline");
                }
                // Baseline firefly animation
                self.update_baseline().await;

                // Re-trigger health override after cooldown (if health still bad)
                self.context.write().await.maybe_retrigger_health();
            }
        }
    }

    /// Update and render baseline firefly animation
    async fn update_baseline(&mut self) {
        // Pixel animations only apply to the RP2040 matrix.
        if self.firefly.kind != FireflyKind::Rp2040Matrix {
            return;
        }

        let ctx = self.context.read().await;
        let activity = ctx.effective_activity();
        let tempo = Tempo::from_load(activity);
        let has_seed_bank = ctx.has_seed_bank;
        let has_services = ctx.has_services;
        let brightness = ctx.brightness;
        drop(ctx);

        self.update_fireflies(&tempo);
        self.maybe_spawn_firefly(&tempo, has_seed_bank, has_services);
        self.render_fireflies(brightness).await;
    }

    /// Update firefly phases and progress
    fn update_fireflies(&mut self, tempo: &Tempo) {
        let now = Instant::now();

        for firefly in &mut self.sprites {
            let elapsed = now.duration_since(firefly.phase_start);
            let phase_duration = match firefly.phase {
                Phase::FadeIn => tempo.fade_in,
                Phase::Peak => tempo.peak,
                Phase::FadeOut => tempo.fade_out,
                Phase::Dormant => Duration::MAX,
            };

            firefly.progress = (elapsed.as_secs_f32() / phase_duration.as_secs_f32()).min(1.0);

            // Transition to next phase
            if firefly.progress >= 1.0 {
                firefly.phase = match firefly.phase {
                    Phase::FadeIn => Phase::Peak,
                    Phase::Peak => Phase::FadeOut,
                    Phase::FadeOut => Phase::Dormant,
                    Phase::Dormant => Phase::Dormant,
                };
                firefly.progress = 0.0;
                firefly.phase_start = now;
            }
        }

        // Remove dormant fireflies and free their positions
        self.sprites.retain(|f| {
            if f.phase == Phase::Dormant {
                let idx = (f.y as usize) * GRID_SIZE as usize + (f.x as usize);
                self.occupied[idx] = false;
                false
            } else {
                true
            }
        });
    }

    /// Spawn a new firefly if conditions are met
    fn maybe_spawn_firefly(&mut self, tempo: &Tempo, has_seed_bank: bool, has_services: bool) {
        // Check spawn interval
        if self.last_spawn.elapsed() < tempo.spawn_interval {
            return;
        }

        // Check concurrent limit
        if self.sprites.len() >= tempo.max_concurrent {
            return;
        }

        // Find available position
        let available: Vec<usize> = self
            .occupied
            .iter()
            .enumerate()
            .filter(|&(_, &occ)| !occ)
            .map(|(i, _)| i)
            .collect();

        if available.is_empty() {
            return;
        }

        // Pick random position
        let idx = available[self.rng.random_range(0..available.len())];
        let x = (idx % GRID_SIZE as usize) as u8;
        let y = (idx / GRID_SIZE as usize) as u8;

        // Pick color - mostly warm white, occasionally seed-bank/service colors
        let color = self.pick_color(has_seed_bank, has_services);

        self.occupied[idx] = true;
        self.sprites.push(Sprite::new_at(x, y, color));
        self.last_spawn = Instant::now();

        tracing::trace!(x, y, "Spawned firefly");
    }

    /// Pick firefly color based on context
    ///
    /// Each accent color gets an independent 10% probability window:
    ///   - Green (seed-bank): rolls 0.00–0.10
    ///   - Blue (services):   rolls 0.10–0.20
    ///
    /// Both can coexist without stealing probability from each other.
    fn pick_color(&mut self, has_seed_bank: bool, has_services: bool) -> (u8, u8, u8) {
        let roll: f32 = self.rng.random();

        // 10% chance for green if seed-bank connected
        if has_seed_bank && roll < 0.10 {
            return STORAGE_GREEN;
        }

        // 10% chance for blue if services running (independent window)
        if has_services && (0.10..0.20).contains(&roll) {
            return SERVICE_BLUE;
        }

        // Default: warm white
        WARM_WHITE
    }

    /// Render current sprites to the device.
    async fn render_fireflies(&mut self, brightness: u8) {
        let brightness_factor = brightness as f32 / 100.0;

        let mut pixels: [(u8, u8, u8); TOTAL_PIXELS] = [(0, 0, 0); TOTAL_PIXELS];
        let mut currently_lit = [false; TOTAL_PIXELS];

        for sprite in &self.sprites {
            let idx = (sprite.y as usize) * GRID_SIZE as usize + (sprite.x as usize);
            let (r, g, b) = sprite.current_rgb();
            let adjusted = (
                (r as f32 * brightness_factor) as u8,
                (g as f32 * brightness_factor) as u8,
                (b as f32 * brightness_factor) as u8,
            );
            if adjusted.0 > 2 || adjusted.1 > 2 || adjusted.2 > 2 {
                pixels[idx] = adjusted;
                currently_lit[idx] = true;
            }
        }

        // Clear pixels that were lit before but aren't now.
        for (idx, (&prev, &curr)) in self.prev_lit.iter().zip(currently_lit.iter()).enumerate() {
            if prev && !curr {
                let x = (idx % GRID_SIZE as usize) as u8;
                let y = (idx / GRID_SIZE as usize) as u8;
                let _ = self.firefly.pixel(x, y, 0, 0, 0).await;
            }
        }
        // Set currently lit pixels.
        for (idx, &(r, g, b)) in pixels.iter().enumerate() {
            if currently_lit[idx] {
                let x = (idx % GRID_SIZE as usize) as u8;
                let y = (idx / GRID_SIZE as usize) as u8;
                let _ = self.firefly.pixel(x, y, r, g, b).await;
            }
        }

        self.prev_lit = currently_lit;
    }

    /// Render a single frame of override animation.
    async fn render_override_frame(&mut self, override_type: &Override) {
        if self.firefly.kind != FireflyKind::Rp2040Matrix {
            return;
        }
        match override_type {
            Override::Tended => {
                if self.swarm_frame == 0 {
                    let _ = self.firefly.animate("sparkle").await;
                }
            }
            Override::HealthWarning => {
                if self.swarm_frame == 0 {
                    let _ = self.firefly.status("warning").await;
                }
            }
            Override::HealthError => {
                if self.swarm_frame == 0 {
                    let _ = self.firefly.status("error").await;
                }
            }
            Override::ServiceStarted => {
                if self.swarm_frame == 0 {
                    let _ = self.firefly.fill(0, 180, 0).await;
                }
            }
            Override::ServiceStopped => {
                // Brief dim then restore
                if self.swarm_frame == 0 {
                    let _ = self.firefly.brightness(20).await;
                } else if self.swarm_frame == 15 {
                    // ~500ms at 30fps
                    let _ = self.firefly.brightness(50).await;
                }
            }
            Override::StorageDetected => {
                self.render_swarm_converge_frame().await;
            }
            Override::StorageRemoved => {
                self.render_swarm_disperse_frame().await;
            }
        }
    }

    /// Render one frame of converging swarm animation (storage connected)
    async fn render_swarm_converge_frame(&mut self) {
        let frame = self.swarm_frame;
        let total_frames = 60;
        let settle_start = 45;

        if frame >= total_frames {
            return; // Animation complete, hold final state
        }

        // Clear previous frame
        let _ = self.firefly.clear().await;

        if frame < settle_start {
            // Phase 1: Swarm converging inward
            let converge_progress = frame as f32 / settle_start as f32;
            let num_fireflies = 8 + (converge_progress * 8.0) as usize;

            for i in 0..num_fireflies {
                let edge_idx = i % EDGE_PIXELS.len();
                let (start_x, start_y) = EDGE_PIXELS[edge_idx];

                let jitter = ((i * 7 + frame) % 3) as f32 * 0.15;
                let move_progress = (converge_progress + jitter).min(1.0);

                let x = start_x as f32
                    + (CENTER.0 as f32 - start_x as f32) * ease_in_out(move_progress);
                let y = start_y as f32
                    + (CENTER.1 as f32 - start_y as f32) * ease_in_out(move_progress);

                let flicker = !(frame + i * 13).is_multiple_of(4);
                if flicker {
                    let brightness =
                        0.5 + 0.5 * ((frame as f32 * 0.3 + i as f32).sin() * 0.5 + 0.5);
                    let (r, g, b) = STORAGE_GREEN;
                    let _ = self
                        .firefly
                        .pixel(
                            x.round() as u8,
                            y.round() as u8,
                            (r as f32 * brightness) as u8,
                            (g as f32 * brightness) as u8,
                            (b as f32 * brightness) as u8,
                        )
                        .await;
                }
            }
        } else {
            // Phase 2: Settled glow at center
            let settle_progress =
                (frame - settle_start) as f32 / (total_frames - settle_start) as f32;
            let pulse = 0.7 + 0.3 * (settle_progress * std::f32::consts::PI * 2.0).sin();

            for dy in -1i8..=1 {
                for dx in -1i8..=1 {
                    let px = (CENTER.0 as i8 + dx).clamp(0, 4) as u8;
                    let py = (CENTER.1 as i8 + dy).clamp(0, 4) as u8;
                    let dist = (dx.abs() + dy.abs()) as f32;
                    let intensity = pulse * (1.0 - dist * 0.3);

                    let (r, g, b) = STORAGE_GREEN;
                    let _ = self
                        .firefly
                        .pixel(
                            px,
                            py,
                            (r as f32 * intensity) as u8,
                            (g as f32 * intensity) as u8,
                            (b as f32 * intensity) as u8,
                        )
                        .await;
                }
            }
        }
    }

    /// Render one frame of dispersing swarm animation (storage removed)
    async fn render_swarm_disperse_frame(&mut self) {
        let frame = self.swarm_frame;
        let total_frames = 45;
        let liftoff_frames = 15;

        if frame >= total_frames {
            // Final clear after animation
            let _ = self.firefly.clear().await;
            return;
        }

        // Clear previous frame
        let _ = self.firefly.clear().await;

        if frame < liftoff_frames {
            // Phase 1: Fireflies lifting off
            let liftoff_progress = frame as f32 / liftoff_frames as f32;

            let center_brightness = 1.0 - liftoff_progress * 0.5;
            let (r, g, b) = STORAGE_AMBER;
            let _ = self
                .firefly
                .pixel(
                    CENTER.0,
                    CENTER.1,
                    (r as f32 * center_brightness) as u8,
                    (g as f32 * center_brightness) as u8,
                    (b as f32 * center_brightness) as u8,
                )
                .await;

            let num_fireflies = (liftoff_progress * 8.0) as usize;
            for i in 0..num_fireflies {
                let angle = (i as f32 / 8.0) * std::f32::consts::PI * 2.0;
                let dist = liftoff_progress * 1.5;
                let x = CENTER.0 as f32 + angle.cos() * dist;
                let y = CENTER.1 as f32 + angle.sin() * dist;

                if (0.0..=4.0).contains(&x) && (0.0..=4.0).contains(&y) {
                    let flicker = !(frame + i * 7).is_multiple_of(3);
                    if flicker {
                        let _ = self
                            .firefly
                            .pixel(x.round() as u8, y.round() as u8, r, g, b)
                            .await;
                    }
                }
            }
        } else {
            // Phase 2: Fireflies dispersing outward
            let disperse_progress =
                (frame - liftoff_frames) as f32 / (total_frames - liftoff_frames) as f32;

            let num_fireflies = 12;
            for i in 0..num_fireflies {
                let angle = (i as f32 / num_fireflies as f32) * std::f32::consts::PI * 2.0
                    + (i as f32 * 0.3);
                let base_dist = 1.5 + disperse_progress * 3.0;
                let dist = base_dist + ((i % 3) as f32 * 0.5);

                let x = CENTER.0 as f32 + angle.cos() * dist;
                let y = CENTER.1 as f32 + angle.sin() * dist;

                let fade = (1.0 - disperse_progress).max(0.0);
                let flicker = !(frame + i * 11).is_multiple_of(4);

                if (0.0..=4.0).contains(&x) && (0.0..=4.0).contains(&y) && flicker && fade > 0.1 {
                    let (r, g, b) = STORAGE_AMBER;
                    let _ = self
                        .firefly
                        .pixel(
                            x.round() as u8,
                            y.round() as u8,
                            (r as f32 * fade) as u8,
                            (g as f32 * fade) as u8,
                            (b as f32 * fade) as u8,
                        )
                        .await;
                }
            }
        }
    }

    /// Clear all pixels.
    async fn clear_all(&self) {
        let _ = self.firefly.clear().await;
    }

    /// Reset firefly state for fresh start after override
    fn reset_fireflies(&mut self) {
        self.sprites.clear();
        self.occupied = [false; TOTAL_PIXELS];
        self.prev_lit = [false; TOTAL_PIXELS];
        self.last_spawn = Instant::now();
    }
}

/// Start the animation engine as a background task
pub fn start_animation(
    connection: Arc<Firefly>,
    context: Arc<RwLock<Animation>>,
) -> tokio::task::JoinHandle<()> {
    let engine = AnimationEngine::new(connection, context);
    tokio::spawn(async move {
        engine.run().await;
    })
}
