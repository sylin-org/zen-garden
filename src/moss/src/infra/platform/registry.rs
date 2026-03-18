//! Windows registry operations via `winreg`.
//!
//! Single module for all registry access in Moss.  Replaces bespoke
//! `reg.exe` invocations with type-safe Rust calls.

use anyhow::{Context, Result};
use winreg::enums::*;
use winreg::RegKey;

// ── Read helpers ────────────────────────────────────────────────

/// Read a `REG_SZ` value from `HKEY_LOCAL_MACHINE`.
pub fn read_hklm_string(subkey: &str, value_name: &str) -> Result<String> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let key = hklm
        .open_subkey(subkey)
        .with_context(|| format!("failed to open registry key: HKLM\\{subkey}"))?;
    let val: String = key
        .get_value(value_name)
        .with_context(|| format!("failed to read value: {value_name}"))?;
    Ok(val)
}

/// Read a `REG_SZ` value from `HKEY_LOCAL_MACHINE`, returning `None` if the
/// key or value does not exist.
pub fn read_hklm_string_opt(subkey: &str, value_name: &str) -> Option<String> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let key = hklm.open_subkey(subkey).ok()?;
    key.get_value(value_name).ok()
}

// ── Write helpers ───────────────────────────────────────────────

/// Write a `REG_SZ` value under `HKEY_LOCAL_MACHINE`.
/// Creates the subkey if it doesn't exist.
pub fn write_hklm_string(subkey: &str, value_name: &str, data: &str) -> Result<()> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let (key, _disposition) = hklm
        .create_subkey(subkey)
        .with_context(|| format!("failed to open/create registry key: HKLM\\{subkey}"))?;
    key.set_value(value_name, &data)
        .with_context(|| format!("failed to write value: {value_name}"))?;
    Ok(())
}

/// Delete a value under `HKEY_LOCAL_MACHINE`.
/// Returns `Ok(())` if the value didn't exist.
pub fn delete_hklm_value(subkey: &str, value_name: &str) -> Result<()> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let key = match hklm.open_subkey_with_flags(subkey, KEY_SET_VALUE) {
        Ok(k) => k,
        Err(_) => return Ok(()), // key doesn't exist
    };
    match key.delete_value(value_name) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e).with_context(|| format!("failed to delete value: {value_name}")),
    }
}

// ── Broadcast env change ────────────────────────────────────────

/// Broadcast `WM_SETTINGCHANGE` so running processes pick up
/// machine-scoped environment variable changes without a reboot.
pub fn broadcast_environment_change() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE,
    };

    // "Environment\0" as a wide string
    let env_wide: Vec<u16> = "Environment\0".encode_utf16().collect();

    unsafe {
        let mut _result: usize = 0;
        SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            0,
            env_wide.as_ptr() as isize,
            SMTO_ABORTIFHUNG,
            5000, // 5 s timeout
            &mut _result,
        );
    }
}

// ── Machine-scoped environment variables ────────────────────────

const MACHINE_ENV_KEY: &str = r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment";

/// Read a machine-scoped environment variable from the registry.
pub fn read_machine_env(var_name: &str) -> Option<String> {
    read_hklm_string_opt(MACHINE_ENV_KEY, var_name)
}

/// Write a machine-scoped environment variable to the registry
/// and broadcast `WM_SETTINGCHANGE`.
pub fn write_machine_env(var_name: &str, value: &str) -> Result<()> {
    write_hklm_string(MACHINE_ENV_KEY, var_name, value)?;
    broadcast_environment_change();
    Ok(())
}

/// Delete a machine-scoped environment variable from the registry
/// and broadcast `WM_SETTINGCHANGE`.
pub fn delete_machine_env(var_name: &str) -> Result<()> {
    delete_hklm_value(MACHINE_ENV_KEY, var_name)?;
    broadcast_environment_change();
    Ok(())
}

// ── Specific registry queries used elsewhere in Moss ────────────

const TCPIP_PARAMS_KEY: &str = r"SYSTEM\CurrentControlSet\Services\Tcpip\Parameters";
const CRYPTOGRAPHY_KEY: &str = r"SOFTWARE\Microsoft\Cryptography";

/// Read the Windows DNS hostname from the registry.
pub fn get_dns_hostname() -> Option<String> {
    read_hklm_string_opt(TCPIP_PARAMS_KEY, "Hostname")
}

/// Set the Windows DNS hostname (both volatile and non-volatile keys).
/// Requires elevation.  Requires reboot to take full effect.
pub fn set_dns_hostname(name: &str) -> Result<()> {
    write_hklm_string(TCPIP_PARAMS_KEY, "Hostname", name).context("failed to set Hostname")?;
    write_hklm_string(TCPIP_PARAMS_KEY, "NV Hostname", name)
        .context("failed to set NV Hostname")?;
    tracing::info!(name = %name, "set Windows DNS hostname (reboot required)");
    Ok(())
}

/// Read the Windows `MachineGuid` from the Cryptography registry key.
pub fn get_machine_guid() -> Result<String> {
    read_hklm_string(CRYPTOGRAPHY_KEY, "MachineGuid").context("failed to read Windows MachineGuid")
}
