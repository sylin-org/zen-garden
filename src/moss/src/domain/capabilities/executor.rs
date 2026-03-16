//! Capability executor implementation
//!
//! Executes manifest-defined commands to discover, add, and remove capabilities.

use crate::docker::zen_offering_container_name;
use anyhow::{bail, Context, Result};
use garden_common::manifests::{
    CapabilityManifest, CapabilityTypeConfig, FieldMappings, ListOperationConfig, ModeCommands,
    OutputFormat,
};
use garden_common::{
    CapabilityCollection, CapabilityDisplay, CapabilityItem, OfferingMode, ServiceInfo,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::process::Stdio;
use tokio::process::Command;

/// Context for capability execution
///
/// Contains all the variables needed to template and execute commands.
pub struct Executor {
    /// Service mode (managed vs adopted)
    pub mode: OfferingMode,

    /// Container name (for managed mode, e.g., "zen-offering-ollama")
    pub container_name: Option<String>,

    /// Service port (for adopted mode HTTP endpoints)
    pub port: u16,
}

/// Result of an add/remove/upgrade operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityMutationResult {
    /// Whether the operation succeeded
    pub success: bool,
    /// Error message if failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// The capability name that was operated on
    pub capability: String,
    /// The operation performed
    pub operation: String,
}

/// Result of checking if a capability has an update available
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityUpdateStatus {
    /// Capability name
    pub name: String,
    /// Capability type
    pub cap_type: String,
    /// Whether an update is available
    pub update_available: bool,
    /// Local version/digest (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_version: Option<String>,
    /// Remote version/digest (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_version: Option<String>,
    /// Error if check failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Capability executor - runs manifest commands to discover, add, and remove capabilities
pub struct CapabilityExecutor;

impl CapabilityExecutor {
    /// Create a new executor
    pub fn new() -> Self {
        Self
    }

    fn build_context(&self, service: &ServiceInfo, mode: OfferingMode) -> Result<Executor> {
        let container_name = match mode {
            OfferingMode::Managed => Some(zen_offering_container_name(&service.name)?),
            _ => None,
        };

        Ok(Executor {
            mode,
            container_name,
            port: service.ports.native,
        })
    }

    /// List capabilities for an offering using its manifest
    ///
    /// # Arguments
    /// * `service` - The service to query
    /// * `manifest` - The capability manifest defining how to query
    /// * `mode` - The offering mode (managed or adopted)
    ///
    /// # Returns
    /// Vector of capability collections (one per capability type)
    pub async fn list_capabilities(
        &self,
        service: &ServiceInfo,
        manifest: &CapabilityManifest,
        mode: OfferingMode,
    ) -> Result<Vec<CapabilityCollection>> {
        let mut collections = Vec::new();

        // Build execution context with templating variables
        let context = self.build_context(service, mode)?;

        // Execute list command for each capability type
        for cap_config in &manifest.capabilities {
            match self
                .list_capability_type(service, cap_config, &context)
                .await
            {
                Ok(collection) => {
                    tracing::debug!(
                        offering = %manifest.offering,
                        cap_type = %cap_config.cap_type,
                        count = collection.items.len(),
                        "Discovered capabilities"
                    );
                    collections.push(collection);
                }
                Err(e) => {
                    tracing::warn!(
                        offering = %manifest.offering,
                        cap_type = %cap_config.cap_type,
                        error = ?e,
                        "Failed to list capability type"
                    );
                    // Continue with other capability types
                }
            }
        }

        Ok(collections)
    }

    /// Add a capability to an offering
    ///
    /// # Arguments
    /// * `service` - The service to add capability to
    /// * `manifest` - The capability manifest
    /// * `mode` - The offering mode
    /// * `cap_type` - The capability type (e.g., "model")
    /// * `capability_name` - The name of the capability to add (e.g., "llama2:7b")
    pub async fn add_capability(
        &self,
        service: &ServiceInfo,
        manifest: &CapabilityManifest,
        mode: OfferingMode,
        cap_type: &str,
        capability_name: &str,
    ) -> Result<CapabilityMutationResult> {
        // Validate capability name
        self.validate_capability_name(capability_name)?;

        // Find the capability type config
        let cap_config = manifest
            .get_capability_type(cap_type)
            .with_context(|| format!("Unknown capability type: {}", cap_type))?;

        // Check if add is available
        let add_config = cap_config
            .add
            .as_ref()
            .with_context(|| format!("Add operation not configured for type: {}", cap_type))?;

        if !add_config.available {
            let reason = add_config
                .reason
                .as_deref()
                .unwrap_or("Operation not available");
            return Ok(CapabilityMutationResult {
                success: false,
                error: Some(reason.to_string()),
                capability: capability_name.to_string(),
                operation: "add".to_string(),
            });
        }

        let commands = add_config
            .commands
            .as_ref()
            .with_context(|| "No commands defined for add operation")?;

        // Build context
        let context = self.build_context(service, mode)?;

        // Get and template command
        let command = self.get_command(commands, &context)?;
        let templated = self.template_command_with_item(&command, &context, capability_name)?;

        tracing::info!(
            service = %service.name,
            cap_type = %cap_type,
            capability = %capability_name,
            command = %templated,
            "Executing capability add command"
        );

        // Execute the command
        match self
            .execute_command(&templated, add_config.timeout_secs, &context)
            .await
        {
            Ok(_output) => {
                tracing::info!(
                    service = %service.name,
                    capability = %capability_name,
                    "Successfully added capability"
                );
                Ok(CapabilityMutationResult {
                    success: true,
                    error: None,
                    capability: capability_name.to_string(),
                    operation: "add".to_string(),
                })
            }
            Err(e) => {
                tracing::warn!(
                    service = %service.name,
                    capability = %capability_name,
                    error = ?e,
                    "Failed to add capability"
                );
                Ok(CapabilityMutationResult {
                    success: false,
                    error: Some(e.to_string()),
                    capability: capability_name.to_string(),
                    operation: "add".to_string(),
                })
            }
        }
    }

    /// Remove a capability from an offering
    ///
    /// # Arguments
    /// * `service` - The service to remove capability from
    /// * `manifest` - The capability manifest
    /// * `mode` - The offering mode
    /// * `cap_type` - The capability type (e.g., "model")
    /// * `capability_name` - The name of the capability to remove
    pub async fn remove_capability(
        &self,
        service: &ServiceInfo,
        manifest: &CapabilityManifest,
        mode: OfferingMode,
        cap_type: &str,
        capability_name: &str,
    ) -> Result<CapabilityMutationResult> {
        // Validate capability name
        self.validate_capability_name(capability_name)?;

        // Find the capability type config
        let cap_config = manifest
            .get_capability_type(cap_type)
            .with_context(|| format!("Unknown capability type: {}", cap_type))?;

        // Check if remove is available
        let remove_config = cap_config
            .remove
            .as_ref()
            .with_context(|| format!("Remove operation not configured for type: {}", cap_type))?;

        if !remove_config.available {
            let reason = remove_config
                .reason
                .as_deref()
                .unwrap_or("Operation not available");
            return Ok(CapabilityMutationResult {
                success: false,
                error: Some(reason.to_string()),
                capability: capability_name.to_string(),
                operation: "remove".to_string(),
            });
        }

        let commands = remove_config
            .commands
            .as_ref()
            .with_context(|| "No commands defined for remove operation")?;

        // Build context
        let context = self.build_context(service, mode)?;

        // Get and template command
        let command = self.get_command(commands, &context)?;
        let templated = self.template_command_with_item(&command, &context, capability_name)?;

        tracing::info!(
            service = %service.name,
            cap_type = %cap_type,
            capability = %capability_name,
            command = %templated,
            "Executing capability remove command"
        );

        // Execute the command
        match self
            .execute_command(&templated, remove_config.timeout_secs, &context)
            .await
        {
            Ok(_output) => {
                tracing::info!(
                    service = %service.name,
                    capability = %capability_name,
                    "Successfully removed capability"
                );
                Ok(CapabilityMutationResult {
                    success: true,
                    error: None,
                    capability: capability_name.to_string(),
                    operation: "remove".to_string(),
                })
            }
            Err(e) => {
                tracing::warn!(
                    service = %service.name,
                    capability = %capability_name,
                    error = ?e,
                    "Failed to remove capability"
                );
                Ok(CapabilityMutationResult {
                    success: false,
                    error: Some(e.to_string()),
                    capability: capability_name.to_string(),
                    operation: "remove".to_string(),
                })
            }
        }
    }

    /// Check if a capability has an update available
    ///
    /// Uses manifest-defined commands to compare local vs remote version/digest.
    ///
    /// # Arguments
    /// * `service` - The service to check
    /// * `manifest` - The capability manifest
    /// * `mode` - The offering mode
    /// * `cap_type` - The capability type (e.g., "model")
    /// * `capability_name` - The name of the capability to check
    pub async fn check_capability_update(
        &self,
        service: &ServiceInfo,
        manifest: &CapabilityManifest,
        mode: OfferingMode,
        cap_type: &str,
        capability_name: &str,
    ) -> Result<CapabilityUpdateStatus> {
        // Validate capability name
        self.validate_capability_name(capability_name)?;

        // Find the capability type config
        let cap_config = manifest
            .get_capability_type(cap_type)
            .with_context(|| format!("Unknown capability type: {}", cap_type))?;

        // Check if check_updates is available
        let check_config = match &cap_config.check_updates {
            Some(c) if c.available => c,
            Some(_) => {
                // Not available - return status indicating check not supported
                return Ok(CapabilityUpdateStatus {
                    name: capability_name.to_string(),
                    cap_type: cap_type.to_string(),
                    update_available: false,
                    local_version: None,
                    remote_version: None,
                    error: Some("Update check not available for this capability type".to_string()),
                });
            }
            None => {
                // No check_updates config - return unknown status
                return Ok(CapabilityUpdateStatus {
                    name: capability_name.to_string(),
                    cap_type: cap_type.to_string(),
                    update_available: false,
                    local_version: None,
                    remote_version: None,
                    error: Some("Update check not configured for this capability type".to_string()),
                });
            }
        };

        // Build context
        let context = self.build_context(service, mode)?;

        // Get local version/digest
        let local_version = if let Some(local_cmd) = &check_config.local_command {
            match self.get_command(local_cmd, &context) {
                Ok(cmd) => {
                    let templated =
                        self.template_command_with_item(&cmd, &context, capability_name)?;
                    match self
                        .execute_command(&templated, check_config.timeout_secs, &context)
                        .await
                    {
                        Ok(output) => self.extract_version(
                            &output,
                            check_config.compare.as_ref().map(|c| c.local_path.as_str()),
                        ),
                        Err(e) => {
                            tracing::debug!(error = ?e, "Failed to get local version");
                            None
                        }
                    }
                }
                Err(_) => None,
            }
        } else {
            None
        };

        // Get remote version/digest
        let remote_version = if let Some(remote_cmd) = &check_config.remote_command {
            match self.get_command(remote_cmd, &context) {
                Ok(cmd) => {
                    let templated =
                        self.template_command_with_item(&cmd, &context, capability_name)?;
                    match self
                        .execute_command(&templated, check_config.timeout_secs, &context)
                        .await
                    {
                        Ok(output) => self.extract_version(
                            &output,
                            check_config
                                .compare
                                .as_ref()
                                .map(|c| c.remote_path.as_str()),
                        ),
                        Err(e) => {
                            tracing::debug!(error = ?e, "Failed to get remote version");
                            None
                        }
                    }
                }
                Err(_) => None,
            }
        } else {
            None
        };

        // Compare versions
        let update_available = match (&local_version, &remote_version) {
            (Some(local), Some(remote)) => local != remote,
            _ => false, // Can't determine without both versions
        };

        Ok(CapabilityUpdateStatus {
            name: capability_name.to_string(),
            cap_type: cap_type.to_string(),
            update_available,
            local_version,
            remote_version,
            error: None,
        })
    }

    /// Upgrade a capability to the latest version
    ///
    /// Semantically distinct from add - used for updating existing capabilities.
    /// Falls back to add command if upgrade command is not defined.
    ///
    /// # Arguments
    /// * `service` - The service to upgrade capability for
    /// * `manifest` - The capability manifest
    /// * `mode` - The offering mode
    /// * `cap_type` - The capability type (e.g., "model")
    /// * `capability_name` - The name of the capability to upgrade
    pub async fn upgrade_capability(
        &self,
        service: &ServiceInfo,
        manifest: &CapabilityManifest,
        mode: OfferingMode,
        cap_type: &str,
        capability_name: &str,
    ) -> Result<CapabilityMutationResult> {
        // Validate capability name
        self.validate_capability_name(capability_name)?;

        // Find the capability type config
        let cap_config = manifest
            .get_capability_type(cap_type)
            .with_context(|| format!("Unknown capability type: {}", cap_type))?;

        // Build context
        let context = self.build_context(service, mode)?;

        // Try upgrade config first, fall back to add config
        let (commands, timeout) = if let Some(upgrade_config) = &cap_config.upgrade {
            if !upgrade_config.available {
                let reason = upgrade_config
                    .reason
                    .as_deref()
                    .unwrap_or("Upgrade not available");
                return Ok(CapabilityMutationResult {
                    success: false,
                    error: Some(reason.to_string()),
                    capability: capability_name.to_string(),
                    operation: "upgrade".to_string(),
                });
            }
            // Use upgrade commands if defined, otherwise fall back to add
            if let Some(cmds) = &upgrade_config.commands {
                (cmds, upgrade_config.timeout_secs)
            } else if let Some(add_config) = &cap_config.add {
                if let Some(cmds) = &add_config.commands {
                    (cmds, upgrade_config.timeout_secs)
                } else {
                    return Ok(CapabilityMutationResult {
                        success: false,
                        error: Some("No commands defined for upgrade operation".to_string()),
                        capability: capability_name.to_string(),
                        operation: "upgrade".to_string(),
                    });
                }
            } else {
                return Ok(CapabilityMutationResult {
                    success: false,
                    error: Some("No commands defined for upgrade operation".to_string()),
                    capability: capability_name.to_string(),
                    operation: "upgrade".to_string(),
                });
            }
        } else if let Some(add_config) = &cap_config.add {
            // No upgrade config, fall back to add (implicit upgrade)
            if !add_config.available {
                let reason = add_config
                    .reason
                    .as_deref()
                    .unwrap_or("Operation not available");
                return Ok(CapabilityMutationResult {
                    success: false,
                    error: Some(reason.to_string()),
                    capability: capability_name.to_string(),
                    operation: "upgrade".to_string(),
                });
            }
            if let Some(cmds) = &add_config.commands {
                (cmds, add_config.timeout_secs)
            } else {
                return Ok(CapabilityMutationResult {
                    success: false,
                    error: Some("No commands defined for upgrade operation".to_string()),
                    capability: capability_name.to_string(),
                    operation: "upgrade".to_string(),
                });
            }
        } else {
            return Ok(CapabilityMutationResult {
                success: false,
                error: Some("Neither upgrade nor add operation configured".to_string()),
                capability: capability_name.to_string(),
                operation: "upgrade".to_string(),
            });
        };

        // Get and template command
        let command = self.get_command(commands, &context)?;
        let templated = self.template_command_with_item(&command, &context, capability_name)?;

        tracing::info!(
            service = %service.name,
            cap_type = %cap_type,
            capability = %capability_name,
            command = %templated,
            "Executing capability upgrade command"
        );

        // Execute the command
        match self.execute_command(&templated, timeout, &context).await {
            Ok(_output) => {
                tracing::info!(
                    service = %service.name,
                    capability = %capability_name,
                    "Successfully upgraded capability"
                );
                Ok(CapabilityMutationResult {
                    success: true,
                    error: None,
                    capability: capability_name.to_string(),
                    operation: "upgrade".to_string(),
                })
            }
            Err(e) => {
                tracing::warn!(
                    service = %service.name,
                    capability = %capability_name,
                    error = ?e,
                    "Failed to upgrade capability"
                );
                Ok(CapabilityMutationResult {
                    success: false,
                    error: Some(e.to_string()),
                    capability: capability_name.to_string(),
                    operation: "upgrade".to_string(),
                })
            }
        }
    }

    /// Check if a capability already exists
    ///
    /// Used to provide early return when trying to add an already-installed capability.
    pub async fn capability_exists(
        &self,
        service: &ServiceInfo,
        manifest: &CapabilityManifest,
        mode: OfferingMode,
        cap_type: &str,
        capability_name: &str,
    ) -> Result<bool> {
        // List current capabilities
        let collections = self.list_capabilities(service, manifest, mode).await?;

        // Find the matching type and check if capability exists
        for collection in collections {
            if collection.cap_type == cap_type {
                return Ok(collection
                    .items
                    .iter()
                    .any(|item| item.name.to_lowercase() == capability_name.to_lowercase()));
            }
        }

        Ok(false)
    }

    /// Extract version/digest from JSON output using a JSONPath
    fn extract_version(&self, output: &str, path: Option<&str>) -> Option<String> {
        let json: serde_json::Value = serde_json::from_str(output).ok()?;

        if let Some(json_path) = path {
            // Simple JSONPath extraction (supports .field.subfield)
            let mut current = &json;
            for part in json_path.trim_start_matches('.').split('.') {
                current = current.get(part)?;
            }
            current.as_str().map(|s| s.to_string())
        } else {
            // No path specified, try common fields
            json.get("digest")
                .or_else(|| json.get("version"))
                .or_else(|| json.get("tag"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        }
    }

    /// Validate capability name to prevent command injection
    fn validate_capability_name(&self, name: &str) -> Result<()> {
        // Max length
        if name.len() > 128 {
            bail!("Capability name too long (max 128 characters)");
        }

        // Valid characters: alphanumeric, underscore, colon, dot, dash, forward slash
        let valid = name.chars().all(|c| {
            c.is_ascii_alphanumeric() || c == '_' || c == ':' || c == '.' || c == '-' || c == '/'
        });

        if !valid {
            bail!("Invalid capability name. Allowed characters: a-z, A-Z, 0-9, _, :, ., -, /");
        }

        // Must not be empty
        if name.is_empty() {
            bail!("Capability name cannot be empty");
        }

        Ok(())
    }

    /// Template command with context variables and item placeholder
    fn template_command_with_item(
        &self,
        command: &str,
        context: &Executor,
        item: &str,
    ) -> Result<String> {
        // Replace {{item}} placeholder FIRST, before template_command validates
        let command_with_item = command.replace("{{item}}", item);

        // Now template the rest of the placeholders
        let result = self.template_command(&command_with_item, context)?;

        Ok(result)
    }

    /// List capabilities for a single capability type
    async fn list_capability_type(
        &self,
        service: &ServiceInfo,
        config: &CapabilityTypeConfig,
        context: &Executor,
    ) -> Result<CapabilityCollection> {
        let list_config = &config.list;

        // Get command for current mode and platform
        let command = self.get_command(&list_config.commands, context)?;

        // Template the command
        let templated = self.template_command(&command, context)?;

        tracing::debug!(
            service = %service.name,
            cap_type = %config.cap_type,
            command = %templated,
            "Executing capability list command"
        );

        // Execute the command
        let output = self
            .execute_command(&templated, list_config.timeout_secs, context)
            .await?;

        // Parse and transform the output
        let items = self.transform_output(&output, list_config).await?;

        // Build collection
        Ok(CapabilityCollection {
            cap_type: config.cap_type.clone(),
            display: CapabilityDisplay {
                singular: config.display.singular.clone(),
                plural: config.display.plural.clone(),
            },
            items,
            discovered_at: chrono::Utc::now(),
        })
    }

    /// Get command string for current mode and platform
    fn get_command(&self, commands: &ModeCommands, context: &Executor) -> Result<String> {
        let platform_commands = match context.mode {
            OfferingMode::Managed => commands.managed.as_ref(),
            OfferingMode::Adopted => commands.adopted.as_ref(),
            OfferingMode::Borrowed => bail!("Borrowed mode does not support capability discovery"),
        };

        let platform_commands = platform_commands
            .with_context(|| format!("No commands defined for {:?} mode", context.mode))?;

        // Get command for current platform
        platform_commands
            .for_current_platform()
            .map(String::from)
            .with_context(|| "No command defined for current platform")
    }

    /// Template command with context variables
    fn template_command(&self, command: &str, context: &Executor) -> Result<String> {
        let mut result = command.to_string();

        // Replace {{container_name}}
        if let Some(ref container_name) = context.container_name {
            result = result.replace("{{container_name}}", container_name);
        }

        // Replace {{port}}
        result = result.replace("{{port}}", &context.port.to_string());

        // Verify no unresolved placeholders remain
        if result.contains("{{") {
            let start = result.find("{{").unwrap();
            let end = result[start..]
                .find("}}")
                .map(|i| start + i + 2)
                .unwrap_or(result.len());
            let placeholder = &result[start..end];
            bail!("Unresolved placeholder in command: {}", placeholder);
        }

        Ok(result)
    }

    /// Execute a shell command
    async fn execute_command(
        &self,
        command: &str,
        timeout_secs: u64,
        _context: &Executor,
    ) -> Result<String> {
        // Use shell to execute command
        #[cfg(target_os = "windows")]
        let (shell, flag) = ("cmd", "/C");

        #[cfg(target_os = "linux")]
        let (shell, flag) = ("sh", "-c");

        let output = tokio::time::timeout(
            std::time::Duration::from_secs(timeout_secs),
            Command::new(shell)
                .arg(flag)
                .arg(command)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output(),
        )
        .await
        .context("Command timed out")?
        .context("Failed to execute command")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            bail!(
                "Command failed with exit code {:?}:\nstdout: {}\nstderr: {}",
                output.status.code(),
                stdout,
                stderr
            );
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Transform raw command output into CapabilityItems
    async fn transform_output(
        &self,
        output: &str,
        config: &ListOperationConfig,
    ) -> Result<Vec<CapabilityItem>> {
        match config.output {
            OutputFormat::Json => self.transform_json(output, config),
            OutputFormat::Lines => self.transform_lines(output),
            OutputFormat::Number => Ok(Vec::new()), // Number format is for summary only
        }
    }

    /// Transform JSON output using transform spec
    fn transform_json(
        &self,
        output: &str,
        config: &ListOperationConfig,
    ) -> Result<Vec<CapabilityItem>> {
        let json: serde_json::Value =
            serde_json::from_str(output).context("Failed to parse command output as JSON")?;

        // Extract items array using items_path
        let items_array = self.extract_path(&json, &config.transform.items_path)?;

        let array = items_array
            .as_array()
            .context("items_path did not resolve to an array")?;

        // Transform each item
        let mut items = Vec::with_capacity(array.len());
        for item in array {
            match self.transform_item(item, &config.transform.fields) {
                Ok(cap_item) => items.push(cap_item),
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to transform item, skipping");
                    continue;
                }
            }
        }

        Ok(items)
    }

    /// Transform line-based output (each line is an item name)
    fn transform_lines(&self, output: &str) -> Result<Vec<CapabilityItem>> {
        let items = output
            .lines()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty())
            .map(CapabilityItem::new)
            .collect();

        Ok(items)
    }

    /// Extract a value from JSON using simple path notation
    fn extract_path(&self, value: &serde_json::Value, path: &str) -> Result<serde_json::Value> {
        let path = path.trim();

        // Handle root
        if path == "." {
            return Ok(value.clone());
        }

        // Remove leading dot if present
        let path = path.strip_prefix('.').unwrap_or(path);

        // Split by dots and navigate
        let mut current = value;
        for segment in path.split('.') {
            let segment = segment.trim();
            if segment.is_empty() {
                continue;
            }

            current = current
                .get(segment)
                .with_context(|| format!("Field '{}' not found in path", segment))?;
        }

        Ok(current.clone())
    }

    /// Transform a single JSON object into a CapabilityItem
    fn transform_item(
        &self,
        item: &serde_json::Value,
        fields: &FieldMappings,
    ) -> Result<CapabilityItem> {
        // Extract required name field
        let name = self
            .extract_path(item, &fields.name)?
            .as_str()
            .context("name field is not a string")?
            .to_string();

        // Extract optional version
        let version = fields
            .version
            .as_ref()
            .and_then(|path| self.extract_path(item, path).ok())
            .and_then(|v| v.as_str().map(String::from));

        // Extract optional size_bytes
        let size_bytes = fields
            .size_bytes
            .as_ref()
            .and_then(|path| self.extract_path(item, path).ok())
            .and_then(|v| v.as_u64());

        // Compute human-readable size from bytes
        let size = size_bytes.map(format_bytes);

        // Extract metadata fields
        let mut metadata = HashMap::new();
        for (key, path) in &fields.metadata {
            if let Ok(value) = self.extract_path(item, path) {
                if !value.is_null() {
                    metadata.insert(key.clone(), value);
                }
            }
        }

        Ok(CapabilityItem {
            name,
            version,
            size,
            size_bytes,
            status: None,
            metadata,
        })
    }
}

impl Default for CapabilityExecutor {
    fn default() -> Self {
        Self::new()
    }
}

/// Format bytes as human-readable string
fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    const TB: u64 = GB * 1024;

    if bytes >= TB {
        format!("{:.1} TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_extract_path_simple() {
        let executor = CapabilityExecutor::new();
        let value = json!({"name": "llama2", "size": 123});

        assert_eq!(
            executor.extract_path(&value, ".name").unwrap(),
            json!("llama2")
        );
        assert_eq!(executor.extract_path(&value, ".size").unwrap(), json!(123));
    }

    #[test]
    fn test_extract_path_nested() {
        let executor = CapabilityExecutor::new();
        let value = json!({
            "details": {
                "family": "llama",
                "quantization": "Q4_0"
            }
        });

        assert_eq!(
            executor.extract_path(&value, ".details.family").unwrap(),
            json!("llama")
        );
    }

    #[test]
    fn test_extract_path_root() {
        let executor = CapabilityExecutor::new();
        let value = json!({"a": 1});
        assert_eq!(executor.extract_path(&value, ".").unwrap(), value);
    }

    #[test]
    fn test_template_command() {
        let executor = CapabilityExecutor::new();
        let context = Executor {
            mode: OfferingMode::Adopted,
            container_name: Some("zen-offering-ollama".to_string()),
            port: 11434,
        };

        let command = "curl -s http://localhost:{{port}}/api/tags";
        let result = executor.template_command(command, &context).unwrap();
        assert_eq!(result, "curl -s http://localhost:11434/api/tags");
    }

    #[test]
    fn test_template_command_container() {
        let executor = CapabilityExecutor::new();
        let context = Executor {
            mode: OfferingMode::Managed,
            container_name: Some("zen-offering-ollama".to_string()),
            port: 11434,
        };

        let command = "docker exec {{container_name}} curl -s http://localhost:11434/api/tags";
        let result = executor.template_command(command, &context).unwrap();
        assert_eq!(
            result,
            "docker exec zen-offering-ollama curl -s http://localhost:11434/api/tags"
        );
    }

    #[test]
    fn test_transform_lines() {
        let executor = CapabilityExecutor::new();
        let output = "llama2\nmistral\ncodellama\n";

        let items = executor.transform_lines(output).unwrap();
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].name, "llama2");
        assert_eq!(items[1].name, "mistral");
        assert_eq!(items[2].name, "codellama");
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(500), "500 B");
        assert_eq!(format_bytes(1500), "1.5 KB");
        assert_eq!(format_bytes(1_500_000), "1.4 MB");
        assert_eq!(format_bytes(3_826_793_472), "3.6 GB");
        assert_eq!(format_bytes(2_000_000_000_000), "1.8 TB");
    }
}
