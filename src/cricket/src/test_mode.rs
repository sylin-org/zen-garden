//! Interactive test mode for tuning
//! Press keys to trigger events and hear the audio

use anyhow::{Context, Result};
use crossterm::{
    cursor::MoveTo,
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    style::{Color, Print, ResetColor, SetForegroundColor},
    terminal::{disable_raw_mode, enable_raw_mode, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::io::{stdout, Write};
use std::sync::Arc;

use crate::manifest::TuneManager;
use crate::mixer::Mixer;

/// Key binding for an event
struct KeyBinding {
    key: char,
    event: String,
    description: String,
}

/// Run interactive test mode
pub async fn run(tune_name: &str, tunes_dir: &str) -> Result<()> {
    // Load tune
    let tune_manager = TuneManager::new(Some(tunes_dir))?;
    tune_manager.select(tune_name)?;
    
    let tune = tune_manager.active()
        .ok_or_else(|| anyhow::anyhow!("Failed to load tune: {}", tune_name))?;
    
    // Initialize mixer
    let mixer = Arc::new(Mixer::new(0.7)?);
    
    // Build key bindings from tune events
    let bindings = build_key_bindings(&tune);
    
    if bindings.is_empty() {
        anyhow::bail!("Tune '{}' has no events defined", tune_name);
    }
    
    // Enter raw mode
    enable_raw_mode().context("Failed to enable raw mode")?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen).context("Failed to enter alternate screen")?;
    
    // Draw UI
    draw_ui(&mut stdout, tune_name, &tune.version, &bindings)?;
    
    // Event loop
    let result = event_loop(&mixer, &tune_manager, &bindings).await;
    
    // Cleanup
    disable_raw_mode().context("Failed to disable raw mode")?;
    execute!(stdout, LeaveAlternateScreen).context("Failed to leave alternate screen")?;
    
    result
}

/// Build key bindings from tune events
fn build_key_bindings(tune: &crate::manifest::TuneManifest) -> Vec<KeyBinding> {
    let mut bindings = Vec::new();
    let keys = "1234567890qwertyuiopasdfghjklzxcvbnm";
    
    for (i, (event, mapping)) in tune.events.iter().enumerate() {
        if i >= keys.len() {
            break;
        }
        
        bindings.push(KeyBinding {
            key: keys.chars().nth(i).unwrap(),
            event: event.clone(),
            description: format!(
                "{} → {} ({})",
                event,
                mapping.resource,
                mapping.channel
            ),
        });
    }
    
    bindings
}

/// Draw the UI
fn draw_ui(
    stdout: &mut std::io::Stdout,
    tune_name: &str,
    version: &str,
    bindings: &[KeyBinding],
) -> Result<()> {
    use crossterm::cursor::MoveTo;
    use crossterm::style::{Color, Print, SetForegroundColor, ResetColor};
    
    execute!(
        stdout,
        MoveTo(0, 0),
        SetForegroundColor(Color::Green),
        Print("╔══════════════════════════════════════════════════════════════╗\n"),
        Print("║              🦗 GARDEN CRICKET - TEST MODE                   ║\n"),
        Print("╚══════════════════════════════════════════════════════════════╝\n"),
        ResetColor,
    )?;
    
    execute!(
        stdout,
        Print(format!("\n  Tune: {} (v{})\n\n", tune_name, version)),
    )?;
    
    execute!(
        stdout,
        SetForegroundColor(Color::Yellow),
        Print("  Key Bindings:\n"),
        ResetColor,
    )?;
    
    for binding in bindings {
        execute!(
            stdout,
            Print(format!("    [{}] {}\n", binding.key, binding.description)),
        )?;
    }
    
    execute!(
        stdout,
        Print("\n"),
        SetForegroundColor(Color::Cyan),
        Print("  Controls:\n"),
        ResetColor,
        Print("    [Space] Stop all audio\n"),
        Print("    [+/-]   Volume up/down\n"),
        Print("    [Esc]   Exit test mode\n"),
        Print("\n"),
    )?;
    
    stdout.flush()?;
    Ok(())
}

/// Main event loop
async fn event_loop(
    mixer: &Arc<Mixer>,
    tune_manager: &TuneManager,
    bindings: &[KeyBinding],
) -> Result<()> {
    let mut stdout = stdout();
    let status_line = (bindings.len() + 15) as u16;
    let mut current_volume: u8 = 70;
    
    loop {
        // Wait for key event
        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(KeyEvent { code, modifiers, .. }) = event::read()? {
                // Handle Ctrl+C
                if code == KeyCode::Char('c') && modifiers.contains(KeyModifiers::CONTROL) {
                    break;
                }
                
                match code {
                    KeyCode::Esc => break,
                    
                    KeyCode::Char(' ') => {
                        // Stop all
                        for channel in [
                            crate::mixer::Channel::Foreground,
                            crate::mixer::Channel::Midground,
                            crate::mixer::Channel::Ambient,
                            crate::mixer::Channel::Background,
                        ] {
                            mixer.stop(channel).await;
                        }
                        show_status(&mut stdout, status_line, "Stopped all audio", Color::Yellow)?;
                    }
                    
                    KeyCode::Char('+') | KeyCode::Char('=') => {
                        current_volume = (current_volume + 10).min(100);
                        mixer.set_master_volume(current_volume as f32 / 100.0).await;
                        show_status(&mut stdout, status_line, 
                            &format!("Volume: {}%", current_volume), Color::Green)?;
                    }
                    
                    KeyCode::Char('-') | KeyCode::Char('_') => {
                        current_volume = current_volume.saturating_sub(10);
                        mixer.set_master_volume(current_volume as f32 / 100.0).await;
                        show_status(&mut stdout, status_line, 
                            &format!("Volume: {}%", current_volume), Color::Green)?;
                    }
                    
                    KeyCode::Char(c) => {
                        // Find matching binding
                        if let Some(binding) = bindings.iter().find(|b| b.key == c) {
                            play_event(mixer, tune_manager, &binding.event).await;
                            show_status(&mut stdout, status_line, 
                                &format!("▶ {}", binding.event), Color::Green)?;
                        }
                    }
                    
                    _ => {}
                }
            }
        }
    }
    
    Ok(())
}

/// Show status message
fn show_status(
    stdout: &mut std::io::Stdout,
    line: u16,
    message: &str,
    color: crossterm::style::Color,
) -> Result<()> {
    use crossterm::cursor::MoveTo;
    use crossterm::style::{Print, SetForegroundColor, ResetColor};
    use crossterm::terminal::{Clear, ClearType};
    
    execute!(
        stdout,
        MoveTo(0, line),
        Clear(ClearType::CurrentLine),
        SetForegroundColor(color),
        Print(format!("  {}", message)),
        ResetColor,
    )?;
    
    stdout.flush()?;
    Ok(())
}

/// Play event audio
async fn play_event(mixer: &Arc<Mixer>, tune_manager: &TuneManager, event: &str) {
    let Some(mapping) = tune_manager.get_event_mapping(event) else {
        return;
    };
    
    let Some(channel) = crate::mixer::Channel::from_str(&mapping.channel) else {
        return;
    };
    
    let active_name = tune_manager.active_name().unwrap_or_default();
    let Some(audio_data) = tune_manager.resolve_resource_bytes_with_fallback(&active_name, &mapping.resource) else {
        return;
    };
    
    let _ = mixer.play_bytes(channel, audio_data, mapping.looping).await;
}
