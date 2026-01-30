//! Firefly animation engine
//!
//! Manages the ambient "firefly" LED animation that serves as the baseline
//! visual state. Events temporarily override this baseline, then it resumes.

use std::sync::Arc;
use std::time::{Duration, Instant};

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use tokio::sync::RwLock;

use crate::serial::FireflyConnection;

/// Matrix dimensions
const GRID_SIZE: u8 = 5;
const TOTAL_PIXELS: usize = 25;

/// Animation runs at this rate
const FRAME_RATE: u64 = 30; // fps
const FRAME_DURATION: Duration = Duration::from_millis(1000 / FRAME_RATE);

/// Warm white color (slightly amber, like real fireflies)
const WARM_WHITE: (u8, u8, u8) = (255, 180, 100);
/// Storage activity indicator
const STORAGE_GREEN: (u8, u8, u8) = (80, 255, 80);
/// Service activity indicator
const SERVICE_BLUE: (u8, u8, u8) = (80, 140, 255);

/// Firefly lifecycle phase
#[derive(Debug, Clone, Copy, PartialEq)]
enum Phase {
    FadeIn,
    Peak,
    FadeOut,
    /// Waiting to spawn (not visible)
    Dormant,
}

/// A single firefly pixel
#[derive(Debug, Clone)]
struct Firefly {
    x: u8,
    y: u8,
    color: (u8, u8, u8),
    phase: Phase,
    /// Progress within current phase (0.0 to 1.0)
    progress: f32,
    /// When this firefly started its current phase
    phase_start: Instant,
}

impl Firefly {
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
}

impl Override {
    /// How long this override lasts
    fn duration(&self) -> Duration {
        match self {
            Override::Tended => Duration::from_secs(3),
            Override::HealthWarning => Duration::from_secs(60), // Re-evaluated by events
            Override::HealthError => Duration::from_secs(60),
            Override::ServiceStarted => Duration::from_millis(1500),
            Override::ServiceStopped => Duration::from_millis(1000),
        }
    }
}

/// Activity bonus per installed offering
const OFFERING_ACTIVITY_BONUS: f32 = 0.05;

/// Shared animation state updated by SSE events
pub struct AnimationContext {
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
    /// Active override (if any)
    pub active_override: Option<(Override, Instant)>,
    /// Whether animation should run
    pub enabled: bool,
    /// User-configured brightness (0-100), persisted
    pub brightness: u8,
    /// State directory for persistence
    state_dir: Option<std::path::PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Health {
    Thriving,
    Withering,
    Wilting,
}

impl Default for Health {
    fn default() -> Self {
        Health::Thriving
    }
}

/// Default brightness (fairly dim for ambient use)
const DEFAULT_BRIGHTNESS: u8 = 30;

impl AnimationContext {
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
            active_override: None,
            enabled: true,
            brightness,
            state_dir,
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
        self.active_override = Some((override_type, Instant::now()));
    }

    /// Clear override (e.g., health recovered)
    pub fn clear_override(&mut self) {
        self.active_override = None;
    }

    /// Check if override has expired
    pub fn update_override(&mut self) {
        if let Some((ref override_type, start)) = self.active_override {
            if start.elapsed() >= override_type.duration() {
                self.active_override = None;
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
            fade_in: Duration::from_secs_f32(5.0 - load * 4.0),      // 5s → 1s
            peak: Duration::from_secs_f32(1.0 - load * 0.7),         // 1s → 0.3s
            fade_out: Duration::from_secs_f32(5.0 - load * 4.0),     // 5s → 1s
            spawn_interval: Duration::from_secs_f32(3.0 - load * 2.8), // 3s → 0.2s
            max_concurrent: 1 + (load * 3.0) as usize,               // 1 → 4
        }
    }
}

/// The animation engine
pub struct AnimationEngine {
    connection: Arc<FireflyConnection>,
    context: Arc<RwLock<AnimationContext>>,
    fireflies: Vec<Firefly>,
    last_spawn: Instant,
    /// Track which pixels are occupied (for spawning logic)
    occupied: [bool; TOTAL_PIXELS],
    /// Track which pixels were lit last frame (for clearing)
    prev_lit: [bool; TOTAL_PIXELS],
    rng: StdRng,
}

impl AnimationEngine {
    pub fn new(
        connection: Arc<FireflyConnection>,
        context: Arc<RwLock<AnimationContext>>,
    ) -> Self {
        Self {
            connection,
            context,
            fireflies: Vec::new(),
            last_spawn: Instant::now(),
            occupied: [false; TOTAL_PIXELS],
            prev_lit: [false; TOTAL_PIXELS],
            rng: StdRng::from_entropy(),
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
                self.clear_all();
                // Wait a bit before checking again
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }

            // Check for override
            let has_override = ctx.active_override.is_some();
            let override_type = ctx.active_override.as_ref().map(|(t, _)| t.clone());
            drop(ctx);

            if has_override {
                // Handle override animation
                if let Some(ref ov) = override_type {
                    self.render_override(ov).await;
                }
                // Update/expire override
                self.context.write().await.update_override();
            } else {
                // Baseline firefly animation
                self.update_baseline().await;
            }
        }
    }

    /// Update and render baseline firefly animation
    async fn update_baseline(&mut self) {
        let ctx = self.context.read().await;
        let activity = ctx.effective_activity();
        let tempo = Tempo::from_load(activity);
        let has_seed_bank = ctx.has_seed_bank;
        let has_services = ctx.has_services;
        let brightness = ctx.brightness;
        drop(ctx);

        // Update existing fireflies
        self.update_fireflies(&tempo);

        // Maybe spawn new firefly
        self.maybe_spawn_firefly(&tempo, has_seed_bank, has_services);

        // Render to device with user brightness
        self.render_fireflies(brightness);
    }

    /// Update firefly phases and progress
    fn update_fireflies(&mut self, tempo: &Tempo) {
        let now = Instant::now();

        for firefly in &mut self.fireflies {
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
        self.fireflies.retain(|f| {
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
        if self.fireflies.len() >= tempo.max_concurrent {
            return;
        }

        // Find available position
        let available: Vec<usize> = self.occupied
            .iter()
            .enumerate()
            .filter(|(_, &occ)| !occ)
            .map(|(i, _)| i)
            .collect();

        if available.is_empty() {
            return;
        }

        // Pick random position
        let idx = available[self.rng.gen_range(0..available.len())];
        let x = (idx % GRID_SIZE as usize) as u8;
        let y = (idx / GRID_SIZE as usize) as u8;

        // Pick color - mostly warm white, occasionally seed-bank/service colors
        let color = self.pick_color(has_seed_bank, has_services);

        self.occupied[idx] = true;
        self.fireflies.push(Firefly::new_at(x, y, color));
        self.last_spawn = Instant::now();

        tracing::trace!(x, y, "Spawned firefly");
    }

    /// Pick firefly color based on context
    fn pick_color(&mut self, has_seed_bank: bool, has_services: bool) -> (u8, u8, u8) {
        let roll: f32 = self.rng.gen();

        // 10% chance for green if seed-bank connected
        if has_seed_bank && roll < 0.10 {
            return STORAGE_GREEN;
        }

        // 10% chance for blue if services running
        if has_services && roll < 0.20 {
            return SERVICE_BLUE;
        }

        // Default: warm white
        WARM_WHITE
    }

    /// Render current fireflies to device
    fn render_fireflies(&mut self, brightness: u8) {
        // Apply user brightness as a multiplier (0-100 -> 0.0-1.0)
        let brightness_factor = brightness as f32 / 100.0;

        // Build pixel buffer with brightness applied
        let mut pixels: [(u8, u8, u8); TOTAL_PIXELS] = [(0, 0, 0); TOTAL_PIXELS];
        let mut currently_lit = [false; TOTAL_PIXELS];

        for firefly in &self.fireflies {
            let idx = (firefly.y as usize) * GRID_SIZE as usize + (firefly.x as usize);
            let (r, g, b) = firefly.current_rgb();
            let adjusted = (
                (r as f32 * brightness_factor) as u8,
                (g as f32 * brightness_factor) as u8,
                (b as f32 * brightness_factor) as u8,
            );
            // Only consider it "lit" if brightness is noticeable
            if adjusted.0 > 2 || adjusted.1 > 2 || adjusted.2 > 2 {
                pixels[idx] = adjusted;
                currently_lit[idx] = true;
            }
        }

        // Send to device
        if let Err(e) = self.connection.with_device(|serial| {
            // Clear pixels that were lit before but aren't now
            for idx in 0..TOTAL_PIXELS {
                if self.prev_lit[idx] && !currently_lit[idx] {
                    let x = (idx % GRID_SIZE as usize) as u8;
                    let y = (idx / GRID_SIZE as usize) as u8;
                    serial.pixel(x, y, 0, 0, 0)?;
                }
            }
            // Set currently lit pixels
            for (idx, &(r, g, b)) in pixels.iter().enumerate() {
                if currently_lit[idx] {
                    let x = (idx % GRID_SIZE as usize) as u8;
                    let y = (idx / GRID_SIZE as usize) as u8;
                    serial.pixel(x, y, r, g, b)?;
                }
            }
            Ok(())
        }) {
            tracing::trace!(error = %e, "Failed to render fireflies");
        }

        // Remember what's lit for next frame
        self.prev_lit = currently_lit;
    }

    /// Render override animation
    async fn render_override(&self, override_type: &Override) {
        match override_type {
            Override::Tended => {
                // Use firmware sparkle animation
                let _ = self.connection.with_device(|serial| serial.animate("sparkle"));
            }
            Override::HealthWarning => {
                // Amber pulse - use firmware or manual
                let _ = self.connection.with_device(|serial| serial.status("warning"));
            }
            Override::HealthError => {
                // Red pulse
                let _ = self.connection.with_device(|serial| serial.status("error"));
            }
            Override::ServiceStarted => {
                // Green bloom - quick fill then fade
                let _ = self.connection.with_device(|serial| serial.fill(0, 180, 0));
            }
            Override::ServiceStopped => {
                // Brief dim
                let _ = self.connection.with_device(|serial| serial.brightness(20));
                tokio::time::sleep(Duration::from_millis(500)).await;
                let _ = self.connection.with_device(|serial| serial.brightness(50));
            }
        }
    }

    /// Clear all pixels
    fn clear_all(&self) {
        let _ = self.connection.with_device(|serial| serial.clear());
    }
}

/// Start the animation engine as a background task
pub fn start_animation(
    connection: Arc<FireflyConnection>,
    context: Arc<RwLock<AnimationContext>>,
) -> tokio::task::JoinHandle<()> {
    let engine = AnimationEngine::new(connection, context);
    tokio::spawn(async move {
        engine.run().await;
    })
}
