//! Windows Explorer shell integration — "Zen Garden" context menu on drives.
//!
//! Registers a cascading right-click menu on removable/external drives:
//!
//! ```text
//! Zen Garden >
//!   ├─ Add Storage to Garden
//!   ├─ ────────────────────
//!   └─ Format and Add Storage   [UAC shield]
//! ```
//!
//! Uses the Windows `SubCommands` + `CommandStore` registry approach — no COM
//! DLLs or shell extensions required.  Moss registers on startup and cleans up
//! on shutdown / uninstall.

use anyhow::{Context, Result};
use tracing::{info, warn};
use winreg::enums::*;
use winreg::RegKey;

// ============================================================================
// Constants
// ============================================================================

/// Registry path for the cascading parent entry.
const SHELL_KEY: &str = r"Drive\shell\ZenGarden";

/// Registry path for the CommandStore verbs.
const COMMAND_STORE: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Explorer\CommandStore\Shell";

/// Verb names (must match the `SubCommands` value).
const VERB_ADOPT: &str = "ZenGarden.Adopt";
const VERB_PREPARE: &str = "ZenGarden.Prepare";

// ============================================================================
// Public API
// ============================================================================

/// Register the "Zen Garden" context menu on all drives.
///
/// Call during Moss startup.  Idempotent — safe to call repeatedly.
pub fn register() -> Result<()> {
    let exe_path = std::env::current_exe()
        .context("Could not resolve exe path for shell integration")?;
    let exe_icon = format!("{},1", exe_path.display());

    // Resolve rake binary — expected next to garden-moss.exe
    let rake_path = exe_path
        .parent()
        .map(|dir| dir.join("garden-rake.exe"))
        .unwrap_or_else(|| std::path::PathBuf::from("garden-rake.exe"));

    let rake_str = rake_path.display().to_string();

    // ── Parent: cascading menu under HKCR\Drive\shell ────────────────
    let hkcr = RegKey::predef(HKEY_CLASSES_ROOT);
    let (parent, _) = hkcr
        .create_subkey(SHELL_KEY)
        .context("Failed to create Drive\\shell\\ZenGarden key")?;

    parent.set_value("MUIVerb", &"Zen Garden")?;
    parent.set_value("Icon", &exe_icon)?;
    parent.set_value(
        "SubCommands",
        &format!("{};{}", VERB_ADOPT, VERB_PREPARE),
    )?;

    // ── Child verbs in CommandStore ───────────────────────────────────
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let (store, _) = hklm
        .create_subkey(COMMAND_STORE)
        .context("Failed to open CommandStore key")?;

    // Verb 1: Add Storage to Garden
    {
        let (verb, _) = store
            .create_subkey(VERB_ADOPT)
            .context("Failed to create Adopt verb")?;
        verb.set_value("MUIVerb", &"Add Storage to Garden")?;
        verb.set_value("Icon", &exe_icon)?;
        let (cmd, _) = verb
            .create_subkey("command")
            .context("Failed to create Adopt command key")?;
        cmd.set_value(
            "",
            &format!(
                "cmd /c \"title Zen Garden && \"{}\" storage adopt --path \"%1\"\"",
                rake_str
            ),
        )?;
    }

    // Verb 2: Format and Add Storage (separator before + UAC shield)
    {
        let (verb, _) = store
            .create_subkey(VERB_PREPARE)
            .context("Failed to create Prepare verb")?;
        verb.set_value("MUIVerb", &"Format and Add Storage")?;

        // ECF_SEPARATORBEFORE = 0x20 — draws a line above this item
        let separator_before: u32 = 0x20;
        verb.set_value("CommandFlags", &separator_before)?;

        // HasLUAShield — adds the UAC shield icon as visual warning
        verb.set_value("HasLUAShield", &"")?;

        let (cmd, _) = verb
            .create_subkey("command")
            .context("Failed to create Prepare command key")?;
        cmd.set_value(
            "",
            &format!(
                "cmd /c \"title Zen Garden && \"{}\" storage prepare --path \"%1\"\"",
                rake_str
            ),
        )?;
    }

    info!("Shell integration: registered Zen Garden context menu on drives");
    Ok(())
}

/// Remove the "Zen Garden" context menu from all drives.
///
/// Call during Moss shutdown or uninstall.  Idempotent — no-ops if already gone.
pub fn unregister() -> Result<()> {
    // Remove parent entry
    let hkcr = RegKey::predef(HKEY_CLASSES_ROOT);
    match hkcr.delete_subkey_all(SHELL_KEY) {
        Ok(()) => info!("Shell integration: removed Drive\\shell\\ZenGarden"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => warn!(error = %e, "Shell integration: failed to remove parent key"),
    }

    // Remove CommandStore verbs
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    if let Ok(store) = hklm.open_subkey_with_flags(COMMAND_STORE, KEY_WRITE) {
        for verb in [VERB_ADOPT, VERB_PREPARE] {
            match store.delete_subkey_all(verb) {
                Ok(()) => info!(verb, "Shell integration: removed CommandStore verb"),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => warn!(verb, error = %e, "Shell integration: failed to remove verb"),
            }
        }
    }

    Ok(())
}
