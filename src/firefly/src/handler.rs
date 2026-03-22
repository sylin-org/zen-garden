//! Command handler for Firefly Companion
//!
//! Implements the SDK's CommandHandler trait for Firefly-specific commands.

use garden_companion_sdk::{CommandHandler, CommandResponse, CompanionState};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::animation::Animation;
use crate::serial::{parse_color, FireflyConnection};

/// Firefly command handler
pub struct FireflyCommands {
    connection: Arc<FireflyConnection>,
    state: Arc<CompanionState>,
    animation: Arc<RwLock<Animation>>,
}

impl FireflyCommands {
    /// Create a new Firefly command handler
    pub fn new(
        connection: Arc<FireflyConnection>,
        state: Arc<CompanionState>,
        animation: Arc<RwLock<Animation>>,
    ) -> Self {
        Self {
            connection,
            state,
            animation,
        }
    }
}

impl CommandHandler for FireflyCommands {
    async fn handle(&self, args: &[String]) -> CommandResponse {
        if args.is_empty() {
            return CommandResponse::error("No command provided").with_suggestions([
                "status <healthy|warning|error|offline>",
                "fill <color>",
                "pixel <x> <y> <color>",
                "animate <rainbow|pulse|chase|sparkle>",
                "brightness <0-100>",
                "clear",
                "stop",
                "info",
            ]);
        }

        let cmd = args[0].to_lowercase();
        let cmd_args = &args[1..];

        match cmd.as_str() {
            // Status indicator
            "status" | "state" => self.handle_status(cmd_args).await,

            // LED control
            "pixel" | "px" => self.handle_pixel(cmd_args).await,
            "fill" => self.handle_fill(cmd_args).await,
            "on" => self.handle_on().await,
            "off" => self.handle_off().await,
            "clear" => self.handle_clear().await,
            "brightness" | "bright" | "dim" => self.handle_brightness(cmd_args).await,

            // Animations
            "animate" | "anim" | "animation" => self.handle_animate(cmd_args).await,
            "stop" => self.handle_stop().await,

            // Info
            "info" => self.handle_info().await,

            _ => CommandResponse::error(format!("Unknown command: {}", cmd)).with_suggestions([
                "status healthy",
                "fill 00ff00",
                "animate rainbow",
                "clear",
                "info",
            ]),
        }
    }

    async fn on_shutdown(&self) {
        tracing::info!("Firefly shutting down");
        let _ = self.connection.with_device(|serial| serial.clear());
    }
}

impl FireflyCommands {
    /// Handle status command
    async fn handle_status(&self, args: &[String]) -> CommandResponse {
        if args.is_empty() {
            return CommandResponse::error("Usage: status <healthy|warning|error|offline>")
                .with_suggestions([
                    "status healthy",
                    "status warning",
                    "status error",
                    "status offline",
                ]);
        }

        let state = args[0].to_lowercase();
        let valid_states = ["healthy", "warning", "error", "offline"];

        if !valid_states.contains(&state.as_str()) {
            return CommandResponse::error(format!("Invalid status: {}", state)).with_suggestions(
                [
                    "status healthy",
                    "status warning",
                    "status error",
                    "status offline",
                ],
            );
        }

        match self.connection.with_device(|serial| serial.status(&state)) {
            Ok(response) => {
                if response.starts_with("OK") {
                    let description = match state.as_str() {
                        "healthy" => "Display showing green (healthy)",
                        "warning" => "Display showing yellow (warning)",
                        "error" => "Display blinking red (error)",
                        "offline" => "Display off (offline)",
                        _ => "Status updated",
                    };
                    CommandResponse::success(description)
                } else {
                    CommandResponse::error(format!("Device error: {}", response))
                }
            }
            Err(e) => CommandResponse::error(format!("Not connected: {}", e)),
        }
    }

    /// Handle pixel command
    async fn handle_pixel(&self, args: &[String]) -> CommandResponse {
        if args.len() < 3 {
            return CommandResponse::error("Usage: pixel <x> <y> <color>").with_suggestions([
                "pixel 2 2 ff0000",
                "pixel 0 0 green",
                "pixel 4 4 255,255,0",
            ]);
        }

        let x: u8 = match args[0].parse() {
            Ok(v) if v <= 4 => v,
            _ => return CommandResponse::error("X must be 0-4"),
        };

        let y: u8 = match args[1].parse() {
            Ok(v) if v <= 4 => v,
            _ => return CommandResponse::error("Y must be 0-4"),
        };

        let (r, g, b) = match parse_color(&args[2]) {
            Ok(c) => c,
            Err(e) => return CommandResponse::error(format!("Invalid color: {}", e)),
        };

        match self
            .connection
            .with_device(|serial| serial.pixel(x, y, r, g, b))
        {
            Ok(response) => {
                if response.starts_with("OK") {
                    CommandResponse::success(format!(
                        "Pixel ({},{}) set to RGB({},{},{})",
                        x, y, r, g, b
                    ))
                } else {
                    CommandResponse::error(format!("Device error: {}", response))
                }
            }
            Err(e) => CommandResponse::error(format!("Not connected: {}", e)),
        }
    }

    /// Handle fill command
    async fn handle_fill(&self, args: &[String]) -> CommandResponse {
        if args.is_empty() {
            return CommandResponse::error("Usage: fill <color>").with_suggestions([
                "fill ff0000",
                "fill green",
                "fill 255,128,0",
            ]);
        }

        let (r, g, b) = match parse_color(&args[0]) {
            Ok(c) => c,
            Err(e) => return CommandResponse::error(format!("Invalid color: {}", e)),
        };

        match self.connection.with_device(|serial| serial.fill(r, g, b)) {
            Ok(response) => {
                if response.starts_with("OK") {
                    CommandResponse::success(format!("Display filled with RGB({},{},{})", r, g, b))
                } else {
                    CommandResponse::error(format!("Device error: {}", response))
                }
            }
            Err(e) => CommandResponse::error(format!("Not connected: {}", e)),
        }
    }

    /// Handle clear command
    async fn handle_clear(&self) -> CommandResponse {
        match self.connection.with_device(|serial| serial.clear()) {
            Ok(response) => {
                if response.starts_with("OK") {
                    CommandResponse::success("Display cleared")
                } else {
                    CommandResponse::error(format!("Device error: {}", response))
                }
            }
            Err(e) => CommandResponse::error(format!("Not connected: {}", e)),
        }
    }

    /// Handle on command - enable SSE event handling and animation
    async fn handle_on(&self) -> CommandResponse {
        self.state.enable();

        // Update animation context
        {
            let mut ctx = self.animation.write().await;
            ctx.enabled = true;
        }

        CommandResponse::success("Firefly enabled - animation running")
    }

    /// Handle off command - disable SSE event handling and clear display
    async fn handle_off(&self) -> CommandResponse {
        self.state.disable();

        // Update animation context (will clear display)
        {
            let mut ctx = self.animation.write().await;
            ctx.enabled = false;
        }

        // Also explicitly clear
        let _ = self.connection.with_device(|serial| serial.clear());

        CommandResponse::success("Firefly disabled - display cleared")
    }

    /// Handle brightness command
    async fn handle_brightness(&self, args: &[String]) -> CommandResponse {
        if args.is_empty() {
            // Show current brightness
            let ctx = self.animation.read().await;
            return CommandResponse::success(format!("Current brightness: {}%", ctx.brightness));
        }

        let percent: u8 = match args[0].parse() {
            Ok(v) if v <= 100 => v,
            _ => return CommandResponse::error("Brightness must be 0-100"),
        };

        // Update animation context (persists to disk)
        {
            let mut ctx = self.animation.write().await;
            ctx.set_brightness(percent);
        }

        CommandResponse::success(format!("Brightness set to {}% (saved)", percent))
    }

    /// Handle animate command
    async fn handle_animate(&self, args: &[String]) -> CommandResponse {
        if args.is_empty() {
            return CommandResponse::error("Usage: animate <rainbow|pulse|chase|sparkle>")
                .with_suggestions([
                    "animate rainbow",
                    "animate pulse",
                    "animate chase",
                    "animate sparkle",
                ]);
        }

        let name = args[0].to_lowercase();
        let valid_anims = ["rainbow", "pulse", "chase", "sparkle"];

        if !valid_anims.contains(&name.as_str()) {
            return CommandResponse::error(format!("Unknown animation: {}", name))
                .with_suggestions([
                    "animate rainbow",
                    "animate pulse",
                    "animate chase",
                    "animate sparkle",
                ]);
        }

        match self.connection.with_device(|serial| serial.animate(&name)) {
            Ok(response) => {
                if response.starts_with("OK") {
                    CommandResponse::success(format!("Playing animation: {}", name))
                } else {
                    CommandResponse::error(format!("Device error: {}", response))
                }
            }
            Err(e) => CommandResponse::error(format!("Not connected: {}", e)),
        }
    }

    /// Handle stop command
    async fn handle_stop(&self) -> CommandResponse {
        match self.connection.with_device(|serial| serial.stop()) {
            Ok(response) => {
                if response.starts_with("OK") {
                    CommandResponse::success("Animation stopped")
                } else {
                    CommandResponse::error(format!("Device error: {}", response))
                }
            }
            Err(e) => CommandResponse::error(format!("Not connected: {}", e)),
        }
    }

    /// Handle info command
    async fn handle_info(&self) -> CommandResponse {
        let status = self.connection.status_info();
        let sse_status = if self.state.is_enabled() { "on" } else { "off" };
        let brightness = self.animation.read().await.brightness;

        if !self.connection.is_connected() {
            return CommandResponse::success_with_details(
                "Firefly Companion running (no device)",
                format!(
                    "Status: {}\nSSE events: {}\nBrightness: {}%\n\nConnect a Waveshare RP2040-Matrix to enable LED control.",
                    status, sse_status, brightness
                ),
            );
        }

        match self.connection.with_device(|serial| serial.info()) {
            Ok(response) => {
                if response.starts_with("OK") {
                    // Parse: OK,firefly-v0,rp2040-matrix,5x5
                    let parts: Vec<&str> = response.split(',').skip(1).collect();

                    let mut details = format!("Status: {}\n", status);
                    details.push_str(&format!("SSE events: {}\n", sse_status));
                    details.push_str(&format!("Brightness: {}%\n", brightness));
                    if parts.len() >= 3 {
                        details.push_str(&format!("Firmware: {}\n", parts[0]));
                        details.push_str(&format!("Hardware: {}\n", parts[1]));
                        details.push_str(&format!("Matrix: {}\n", parts[2]));
                    } else {
                        details.push_str(&format!("Raw: {}\n", response));
                    }

                    CommandResponse::success_with_details("Firefly device connected", details)
                } else {
                    CommandResponse::error(format!("Device error: {}", response))
                }
            }
            Err(e) => CommandResponse::error(format!("Communication error: {}", e)),
        }
    }
}
