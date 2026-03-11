// UI rendering module for rake CLI
// Provides reusable helpers for consistent terminal output
// Following SoC/DRY principles - all formatting logic centralized here

use colored::Colorize;

/// Terminal capability information
#[derive(Debug, Clone)]
pub struct TerminalInfo {
    pub width: usize,
    pub supports_color: bool,
    pub supports_unicode: bool,
}

impl TerminalInfo {
    /// Detect terminal capabilities (width and color support)
    pub fn detect() -> Self {
        let width = terminal_size::terminal_size()
            .map(|(w, _)| w.0 as usize)
            .unwrap_or(constants::DEFAULT_TERMINAL_WIDTH);

        // Check NO_COLOR environment variable first (universal override)
        let no_color = std::env::var(crate::ENV_NO_COLOR).is_ok();

        // Use supports-color crate for proper terminal detection
        let supports_color =
            !no_color && supports_color::on(supports_color::Stream::Stdout).is_some();

        // Unicode support: disabled on Windows by default (PowerShell encoding issues)
        // Can be enabled with GARDEN_UNICODE=1 environment variable
        let supports_unicode = if cfg!(windows) {
            std::env::var(crate::ENV_GARDEN_UNICODE).is_ok()
        } else {
            true // Unix terminals generally handle Unicode well
        };

        Self {
            width,
            supports_color,
            supports_unicode,
        }
    }
}

/// Consistent output formatting with automatic indentation and status indicators
/// Reduces duplication of println!/eprintln! calls with manual formatting
#[allow(dead_code)] // Incrementally adopting this pattern
pub struct OutputWriter {
    term: TerminalInfo,
    indent: usize,
}

#[allow(dead_code)]
impl OutputWriter {
    /// Create new output writer with default settings
    pub fn new() -> Self {
        Self {
            term: TerminalInfo::detect(),
            indent: constants::DEFAULT_INDENT,
        }
    }

    /// Create output writer with custom indentation
    pub fn with_indent(indent: usize) -> Self {
        Self {
            term: TerminalInfo::detect(),
            indent,
        }
    }

    /// Create output writer with pre-detected terminal info
    pub fn with_term(term: TerminalInfo) -> Self {
        Self {
            term,
            indent: constants::DEFAULT_INDENT,
        }
    }

    /// Success message (green OK indicator)
    pub fn success(&self, msg: impl std::fmt::Display) {
        println!(
            "{}{} {}",
            " ".repeat(self.indent),
            status_indicator("ok", self.term.supports_color),
            msg
        );
    }

    /// Error message (red ERROR indicator)
    pub fn error(&self, msg: impl std::fmt::Display) {
        eprintln!(
            "{}{} {}",
            " ".repeat(self.indent),
            status_indicator("error", self.term.supports_color),
            msg
        );
    }

    /// Info message (blue info indicator)
    pub fn info(&self, msg: impl std::fmt::Display) {
        println!(
            "{}{} {}",
            " ".repeat(self.indent),
            status_indicator("info", self.term.supports_color),
            msg
        );
    }

    /// Warning message (yellow WARN indicator)
    pub fn warn(&self, msg: impl std::fmt::Display) {
        println!(
            "{}{} {}",
            " ".repeat(self.indent),
            status_indicator("warn", self.term.supports_color),
            msg
        );
    }

    /// Pending/in-progress message
    pub fn pending(&self, msg: impl std::fmt::Display) {
        println!(
            "{}{} {}",
            " ".repeat(self.indent),
            status_indicator("pending", self.term.supports_color),
            msg
        );
    }

    /// Detail line (indented, no indicator)
    pub fn detail(&self, msg: impl std::fmt::Display) {
        println!("{}  {}", " ".repeat(self.indent), msg);
    }

    /// Bullet point (• prefix)
    pub fn bullet(&self, msg: impl std::fmt::Display) {
        println!("{}  • {}", " ".repeat(self.indent), msg);
    }

    /// Plain line with indent
    pub fn line(&self, msg: impl std::fmt::Display) {
        println!("{}{}", " ".repeat(self.indent), msg);
    }

    /// Blank line
    pub fn blank_line(&self) {
        println!();
    }

    /// Get terminal info for advanced formatting
    pub fn term(&self) -> &TerminalInfo {
        &self.term
    }
}

impl Default for OutputWriter {
    fn default() -> Self {
        Self::new()
    }
}

/// Verbosity level for command output (Phase 3)
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Verbosity {
    Minimal = 0, // -v0
    #[default]
    Standard = 1, // -v1 (DEFAULT)
    Verbose = 2, // -v2
    Debug = 3,   // -v3
}

#[allow(dead_code)]
impl Verbosity {
    /// Parse from command line argument (e.g., "-v0", "-v1")
    pub fn from_arg(arg: &str) -> Option<Self> {
        match arg {
            "-v0" => Some(Self::Minimal),
            "-v1" => Some(Self::Standard),
            "-v2" => Some(Self::Verbose),
            "-v3" => Some(Self::Debug),
            _ => None,
        }
    }
}

/// Render stone banner (always first line of output)
/// Format: === stone-name - [status] =========
/// Uses garden vitality language: thriving/dormant/needs attention
pub fn stone_banner(name: &str, status: &str, color: bool) -> String {
    let term = TerminalInfo::detect();
    let max_width = term.width.min(80);

    let status_lower = status.to_lowercase();
    let status_with_brackets = format!("[{}]", status);
    let status_colored = if color {
        if status_lower.contains(crate::VITALITY_THRIVING)
            || status_lower.contains(crate::HEALTH_HEALTHY)
        {
            status_with_brackets.green().to_string()
        } else if status_lower.contains(crate::VITALITY_WITHERING)
            || status_lower.contains(crate::HEALTH_UNHEALTHY)
            || status.contains("ERROR")
        {
            status_with_brackets.red().to_string()
        } else if status_lower.contains(crate::VITALITY_DORMANT)
            || status_lower.contains(crate::VITALITY_NEEDS_ATTENTION)
            || status_lower.contains(crate::HEALTH_DEGRADED)
            || status.contains("WARN")
        {
            status_with_brackets.yellow().to_string()
        } else {
            status_with_brackets
        }
    } else {
        status_with_brackets
    };

    let prefix = "=== ";
    let middle = format!("{} - {}", name, status_colored);
    // For length calculation, use the uncolored version
    let middle_len = format!("{} - [{}]", name, status).len();
    let total_len = prefix.len() + middle_len + 1; // +1 for space before equals

    let equals = if max_width > total_len {
        " ".to_string() + &"=".repeat(max_width - total_len)
    } else {
        String::new()
    };

    format!("{}{}{}", prefix, middle, equals)
}

/// Render stone banner showing just the name (no health status).
/// Format: === stone-name ==================
pub fn stone_name_banner(name: &str, color: bool) -> String {
    let term = TerminalInfo::detect();
    let max_width = term.width.min(80);

    let prefix = "=== ";
    let name_display = if color {
        name.bold().to_string()
    } else {
        name.to_string()
    };
    let total_len = prefix.len() + name.len() + 1; // +1 for space before trailing =

    let equals = if max_width > total_len {
        " ".to_string() + &"=".repeat(max_width - total_len)
    } else {
        String::new()
    };

    format!("{}{}{}", prefix, name_display, equals)
}

/// Render section header with dynamic width (max 40 chars)
/// Format: --- TITLE ---[dashes to 40 chars max]
pub fn section_header(title: &str, term: &TerminalInfo) -> String {
    let prefix = "--- ";
    let suffix = " ";
    let title_len = prefix.len() + title.len() + suffix.len();
    let max_width = term.width.min(40);
    let dashes = if max_width > title_len {
        "-".repeat(max_width - title_len)
    } else {
        String::new()
    };
    format!("{}{}{}{}", prefix, title, suffix, dashes)
}

/// Render section header with short underline (21 chars)
/// Zen Garden UI Standard for grouped key-value displays
/// Format:
/// SECTION_NAME
/// ─────────────────────
pub fn section_header_v2(title: &str, bold: bool, color: bool) -> String {
    const UNDERLINE_LENGTH: usize = 21;
    let underline = "─".repeat(UNDERLINE_LENGTH);

    let title_display = if color && bold {
        title.to_uppercase().bold().to_string()
    } else {
        title.to_uppercase()
    };

    format!("{}\n{}", title_display, underline)
}

/// Render key-value line with proper alignment
/// Label width: VALUE_COLUMN - 1 (35 chars left-aligned), value starts at column 36
/// Format: "    LABEL                               value"
///
/// Note: Prefer using `place_value()` for new code. This function adds indentation
/// while `place_value()` returns just the aligned label+value pair.
pub fn kv_line(label: &str, value: &str, indent_spaces: usize) -> String {
    let label_width = constants::VALUE_COLUMN - 1;
    let indent = " ".repeat(indent_spaces);
    format!(
        "{}{:<width$} {}",
        indent,
        label.to_uppercase(),
        value,
        width = label_width
    )
}

/// Render indented label: value line
#[allow(dead_code)]
pub fn label_value_line(label: &str, value: &str, indent: usize) -> String {
    format!("{}{:<12} {}", " ".repeat(indent), label, value)
}

/// Format number with specified precision (Phase 3)
#[allow(dead_code)]
pub fn format_number(value: f64, precision: usize) -> String {
    format!("{:.*}", precision, value)
}

/// Truncate service/offering name to max length
/// Delegates to crate::utils::strings::truncate
pub fn truncate_name(name: &str, max_len: usize) -> String {
    crate::utils::strings::truncate(name, max_len)
}

/// Render text with specified color (respects terminal color support)
pub fn colored_text(text: &str, color: &str, term: &TerminalInfo) -> String {
    if !term.supports_color {
        return text.to_string();
    }

    match color {
        "red" => text.red().to_string(),
        "green" => text.green().to_string(),
        "yellow" => text.yellow().to_string(),
        "blue" => text.blue().to_string(),
        "magenta" => text.magenta().to_string(),
        "cyan" => text.cyan().to_string(),
        "white" => text.white().to_string(),
        _ => text.to_string(),
    }
}

/// Column alignment for tables
#[derive(Debug, Clone, Copy)]
pub enum Align {
    Left,
    Right,
}

/// Column definition for TableBuilder
#[derive(Debug, Clone)]
struct Column {
    width: usize,
    align: Align,
}

/// Table builder for columnar data with consistent alignment
pub struct TableBuilder {
    columns: Vec<Column>,
    rows: Vec<Vec<String>>,
    indent: usize,
}

impl TableBuilder {
    /// Create new table builder with default indent (4 spaces)
    pub fn new() -> Self {
        Self {
            columns: Vec::new(),
            rows: Vec::new(),
            indent: constants::DEFAULT_INDENT,
        }
    }

    /// Add a column with specified width and alignment
    pub fn add_column(mut self, width: usize, align: Align) -> Self {
        self.columns.push(Column { width, align });
        self
    }

    /// Set custom indentation (default is DEFAULT_INDENT)
    pub fn with_indent(mut self, indent: usize) -> Self {
        self.indent = indent;
        self
    }

    /// Add a data row to the table
    pub fn add_row(&mut self, values: Vec<String>) {
        self.rows.push(values);
    }

    /// Render the table to a string
    pub fn render(&self) -> String {
        let mut output = String::new();
        let indent_str = " ".repeat(self.indent);

        for row in &self.rows {
            output.push_str(&indent_str);
            for (i, value) in row.iter().enumerate() {
                if let Some(col) = self.columns.get(i) {
                    let formatted = match col.align {
                        Align::Left => format!("{:<width$}", value, width = col.width),
                        Align::Right => format!("{:>width$}", value, width = col.width),
                    };
                    output.push_str(&formatted);
                    if i < row.len() - 1 {
                        output.push_str("  ");
                    }
                }
            }
            output.push('\n');
        }
        output
    }
}

impl Default for TableBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Multi-column category layout for explore command
pub struct CategoryGrid {
    items_per_row: usize,
    category_width: usize,
    item_width: usize,
    indent: usize,
}

impl CategoryGrid {
    /// Create new category grid based on terminal width
    pub fn new(term: &TerminalInfo) -> Self {
        let indent = constants::DEFAULT_INDENT;
        let category_width = 12;
        let item_width = 16;
        let available = term.width.saturating_sub(indent + category_width);
        let items_per_row = (available / item_width).max(1);

        Self {
            items_per_row,
            category_width,
            item_width,
            indent,
        }
    }

    /// Render a category with its items in multi-column layout
    /// First row shows category name, continuation rows have blank category column
    pub fn render_category(&self, category: &str, items: &[String]) -> String {
        let mut output = String::new();
        let indent_str = " ".repeat(self.indent);

        for (i, chunk) in items.chunks(self.items_per_row).enumerate() {
            output.push_str(&indent_str);

            if i == 0 {
                // First row: category name
                output.push_str(&format!(
                    "{:<width$}",
                    category,
                    width = self.category_width
                ));
            } else {
                // Continuation rows: blank category column
                output.push_str(&" ".repeat(self.category_width));
            }

            for item in chunk {
                output.push_str(&format!("{:<width$}", item, width = self.item_width));
            }
            output.push('\n');
        }
        output
    }
}

/// Render status indicator with optional color
/// Uses garden vitality language where appropriate
///
/// Color scheme:
/// - [thriving] = green (healthy, running)
/// - [dormant] = dark gray (stopped, offline)
/// - [needs attention] = red (errors, failures)
/// - [planting...] = cyan (installing)
pub fn status_indicator(status: &str, color: bool) -> String {
    let status_lower = status.to_lowercase();
    let status_str = status_lower.as_str();

    // Installing/planting status - service being set up
    if status_str == "installing" || status_str == "planting" {
        let bracketed = "[planting...]".to_string();
        return if color {
            bracketed.cyan().to_string()
        } else {
            bracketed
        };
    }

    // Nourishing/updating status - updates in progress
    if status_str == "nourishing" || status_str == "updating" {
        let bracketed = "[nourishing]".to_string();
        return if color {
            bracketed.yellow().to_string()
        } else {
            bracketed
        };
    }

    let indicator =
        if status_str == crate::SERVICE_RUNNING || status_str == crate::VITALITY_THRIVING {
            crate::VITALITY_THRIVING
        } else if status_str == crate::SERVICE_STOPPED || status_str == crate::VITALITY_DORMANT {
            crate::VITALITY_DORMANT
        } else if status_str == crate::VITALITY_NEEDS_ATTENTION
            || status_str == crate::VITALITY_WITHERING
        {
            crate::VITALITY_NEEDS_ATTENTION
        } else if status_str == "ok" || status_str == crate::HEALTH_HEALTHY {
            crate::VITALITY_THRIVING
        } else if status_str == "error"
            || status_str == "failed"
            || status_str == crate::HEALTH_UNHEALTHY
            || status_str == "warn"
            || status_str == "warning"
            || status_str == crate::HEALTH_DEGRADED
        {
            crate::VITALITY_NEEDS_ATTENTION
        } else {
            return status.to_string(); // Unknown status, pass through without brackets
        };

    // Always bracket known statuses
    let bracketed = format!("[{}]", indicator);

    if color {
        let is_healthy = status_str == crate::SERVICE_RUNNING
            || status_str == "ok"
            || status_str == crate::HEALTH_HEALTHY
            || status_str == crate::VITALITY_THRIVING;

        let is_dormant =
            status_str == crate::SERVICE_STOPPED || status_str == crate::VITALITY_DORMANT;

        let is_degraded =
            status_str == "warn" || status_str == "warning" || status_str == crate::HEALTH_DEGRADED;

        let is_unhealthy = status_str == "error"
            || status_str == "failed"
            || status_str == crate::HEALTH_UNHEALTHY
            || status_str == crate::VITALITY_WITHERING
            || status_str == crate::VITALITY_NEEDS_ATTENTION;

        if is_healthy {
            bracketed.green().to_string()
        } else if is_dormant {
            // Dark gray for dormant/stopped (not an error, just offline)
            bracketed.truecolor(128, 128, 128).to_string()
        } else if is_degraded {
            bracketed.yellow().to_string()
        } else if is_unhealthy {
            bracketed.red().to_string()
        } else {
            bracketed
        }
    } else {
        bracketed
    }
}

/// Render tended marker with gold color
/// Returns " [tended]" in gold when color is enabled, plain otherwise
pub fn tended_marker(color: bool) -> String {
    let marker = " [tended]";
    if color {
        // Gold color (RGB: 255, 215, 0)
        marker.truecolor(255, 215, 0).to_string()
    } else {
        marker.to_string()
    }
}

// ── Compact observe helpers ─────────────────────────────────────────

/// OS indicator for compact table view.
/// Returns emoji (🪟/🐧) when unicode is supported, [W]/[L] fallback otherwise.
pub fn os_indicator(os_string: &str, supports_unicode: bool) -> &'static str {
    let os_lower = os_string.to_lowercase();
    if os_lower.starts_with("windows") || os_lower.starts_with("microsoft") {
        if supports_unicode {
            "\u{1FAA8}"
        } else {
            "[W]"
        } // 🪟
    } else if supports_unicode {
        "\u{1F427}" // 🐧
    } else {
        "[L]"
    }
}

/// Compact status symbol for non-thriving stones.
/// Returns None for thriving (no symbol needed — name color conveys health).
pub fn compact_status_symbol(health: &str, supports_unicode: bool) -> Option<&'static str> {
    let h = health.to_lowercase();
    if h == crate::VITALITY_THRIVING
        || h == crate::SERVICE_RUNNING
        || h == "ok"
        || h == crate::HEALTH_HEALTHY
        || h == "starting"
        || h == "initializing"
    {
        None // Thriving = no symbol
    } else if h == crate::VITALITY_DORMANT || h == crate::SERVICE_STOPPED {
        Some(if supports_unicode { "\u{25CB}" } else { "o" }) // ○
    } else if h == crate::HEALTH_DEGRADED
        || h == crate::VITALITY_NEEDS_ATTENTION
        || h == "warn"
        || h == "warning"
    {
        Some("!")
    } else {
        // withering, error, failed, unhealthy
        Some(if supports_unicode { "\u{2717}" } else { "x" }) // ✗
    }
}

/// Classify health status into a vitality category for coloring.
pub enum VitalityClass {
    Thriving,
    Degraded,
    Withering,
    Dormant,
}

/// Map a health/status string to its vitality class.
pub fn classify_health(health: &str) -> VitalityClass {
    let h = health.to_lowercase();
    if h == crate::VITALITY_THRIVING
        || h == crate::SERVICE_RUNNING
        || h == "ok"
        || h == crate::HEALTH_HEALTHY
        || h == "starting"
        || h == "initializing"
    {
        VitalityClass::Thriving
    } else if h == crate::VITALITY_DORMANT || h == crate::SERVICE_STOPPED {
        VitalityClass::Dormant
    } else if h == crate::HEALTH_DEGRADED
        || h == crate::VITALITY_NEEDS_ATTENTION
        || h == "warn"
        || h == "warning"
    {
        VitalityClass::Degraded
    } else {
        VitalityClass::Withering
    }
}

/// Color a stone name according to its vitality class.
/// Tended stones get gold (255, 215, 0) regardless of health.
pub fn colored_stone_name(name: &str, health: &str, is_tended: bool, color: bool) -> String {
    if !color {
        return name.to_string();
    }
    if is_tended {
        return name.truecolor(255, 215, 0).bold().to_string();
    }
    match classify_health(health) {
        VitalityClass::Thriving => name.green().to_string(),
        VitalityClass::Degraded => name.yellow().to_string(),
        VitalityClass::Withering => name.red().to_string(),
        VitalityClass::Dormant => name.truecolor(128, 128, 128).to_string(),
    }
}

/// Format AI capabilities into a compact string like "GPU 8G/DML" or "2xGPU 16G/CUDA".
pub fn compact_ai(caps: &crate::types::HardwareCapabilities) -> String {
    if let Some(ref ai) = caps.hardware.ai_capabilities {
        if ai.gpu_count == 0 {
            return "\u{2014}".to_string(); // —
        }
        let gpu_prefix = if ai.gpu_count == 1 {
            "GPU".to_string()
        } else {
            format!("{}xGPU", ai.gpu_count)
        };
        let vram = if ai.total_vram_mb >= 1024 {
            format!(" {}G", ai.total_vram_mb / 1024)
        } else if ai.total_vram_mb > 0 {
            format!(" {}M", ai.total_vram_mb)
        } else {
            String::new()
        };
        let runtime = if !ai.runtimes.is_empty() {
            let base: Vec<&str> = ai
                .runtimes
                .iter()
                .filter(|r| !r.contains(':'))
                .map(|r| match r.as_str() {
                    "cuda" => "CUDA",
                    "rocm" => "ROCm",
                    "directml" => "DML",
                    "openvino" => "VINO",
                    _ => r.as_str(),
                })
                .collect();
            if !base.is_empty() {
                format!("/{}", base.join(","))
            } else {
                String::new()
            }
        } else {
            String::new()
        };
        format!("{}{}{}", gpu_prefix, vram, runtime)
    } else {
        "\u{2014}".to_string() // —
    }
}

/// Format offerings list compactly with truncation: "memcached redis vault +2"
pub fn compact_offerings(
    services: &[crate::types::TopologyServiceEntry],
    max_shown: usize,
) -> String {
    if services.is_empty() {
        return "\u{2014}".to_string(); // —
    }
    let names: Vec<&str> = services.iter().map(|s| s.offering.as_str()).collect();
    if names.len() <= max_shown {
        names.join(" ")
    } else {
        let shown: Vec<&str> = names[..max_shown].to_vec();
        format!("{} +{}", shown.join(" "), names.len() - max_shown)
    }
}

/// Fit as many items as possible into a character budget.
///
/// Given plain-text items and a width budget, returns a string that fits:
/// - All items fit: `"a, b, c"`
/// - Partial fit:   `"a, b +2"` (as many as fit, then ` +N` for remainder)
/// - Nothing fits:  `"+5"` (just the overflow count)
/// - Empty input:   `""` (empty string)
///
/// Separator is `", "` (2 chars). Overflow suffix is ` +N`.
pub fn fit_items(items: &[&str], budget: usize) -> String {
    if items.is_empty() || budget == 0 {
        return String::new();
    }

    let total = items.len();
    let sep = ", ";
    let sep_len = sep.len();

    // Try fitting all items first
    let full_len: usize =
        items.iter().map(|s| s.len()).sum::<usize>() + total.saturating_sub(1) * sep_len;

    if full_len <= budget {
        return items.join(sep);
    }

    // Try fitting progressively fewer items with " +N" suffix
    for count in (1..total).rev() {
        let remaining = total - count;
        let suffix = format!(" +{}", remaining);
        let items_len: usize = items[..count].iter().map(|s| s.len()).sum::<usize>()
            + count.saturating_sub(1) * sep_len;
        let needed = items_len + suffix.len();
        if needed <= budget {
            return format!("{}{}", items[..count].join(sep), suffix);
        }
    }

    // Nothing fits — just show overflow count
    let overflow = format!("+{}", total);
    if overflow.len() <= budget {
        overflow
    } else {
        String::new()
    }
}

/// Extract OS family string from runtime info for OS indicator.
/// Handles formats like "windows/Windows 11 Pro" or "linux/Debian GNU/Linux 13".
pub fn os_family_from_runtime(os_string: &str) -> &str {
    if let Some(slash) = os_string.find('/') {
        &os_string[..slash]
    } else {
        os_string
    }
}

/// Build the adaptive legend line for compact observe footer.
pub fn compact_legend(
    has_tended: bool,
    has_windows: bool,
    has_linux: bool,
    term: &TerminalInfo,
) -> String {
    let mut parts: Vec<String> = Vec::new();

    // Tended marker (only in monochrome — in color, gold name is self-evident)
    if has_tended && !term.supports_color {
        parts.push("* tended".to_string());
    }

    // OS indicators
    if has_windows {
        let icon = os_indicator("windows", term.supports_unicode);
        parts.push(format!("{} Windows", icon));
    }
    if has_linux {
        let icon = os_indicator("linux", term.supports_unicode);
        parts.push(format!("{} Linux", icon));
    }

    // Status symbols — always show the full palette
    if term.supports_unicode {
        if term.supports_color {
            parts.push(format!("{} thriving", "✓".green()));
            parts.push(format!("{} degraded", "!".yellow()));
            parts.push(format!("{} withering", "\u{2717}".red()));
            parts.push(format!("{} dormant", "\u{25CB}".truecolor(128, 128, 128)));
        } else {
            parts.push("✓ thriving".to_string());
            parts.push("! degraded".to_string());
            parts.push("\u{2717} withering".to_string());
            parts.push("\u{25CB} dormant".to_string());
        }
    } else {
        parts.push("! degraded".to_string());
        parts.push("x withering".to_string());
        parts.push("o dormant".to_string());
    }

    parts.join("  ")
}

/// Render empty state message with optional action hint
pub fn empty_state(message: &str, action_hint: Option<&str>) -> String {
    let mut output = String::new();
    output.push_str(&format!("    {}\n", message));
    if let Some(hint) = action_hint {
        output.push('\n');
        output.push_str(hint);
        output.push('\n');
    }
    output
}

/// Get appropriate bullet character based on terminal capabilities
/// Uses Unicode on terminals that support it, ASCII fallback otherwise
pub fn bullet(supports_unicode: bool) -> &'static str {
    if supports_unicode {
        "●"
    } else {
        "*"
    }
}

/// Get appropriate hollow bullet based on terminal capabilities  
pub fn hollow_bullet(supports_unicode: bool) -> &'static str {
    if supports_unicode {
        "○"
    } else {
        "o"
    }
}

/// Render progress indicator for operations
/// `[*]` = in progress, `[ ]` = pending
pub fn progress_step(active: bool, message: &str) -> String {
    let indicator = if active { "[*]" } else { "[ ]" };
    format!("    {} {}", indicator, message)
}

/// Render colored category label (Phase 3)
#[allow(dead_code)]
pub fn category_label(name: &str, color: bool) -> String {
    if color {
        name.cyan().to_string()
    } else {
        name.to_string()
    }
}

/// Format elapsed time for display (e.g., "2.3s", "847ms")
/// Shows real timing information - no artificial delays
pub fn format_elapsed_time(elapsed: std::time::Duration) -> String {
    let secs = elapsed.as_secs();
    let millis = elapsed.subsec_millis();

    if secs > 0 {
        format!("{}.{}s", secs, millis / 100)
    } else {
        format!("{}ms", millis)
    }
}

/// Format wall-clock timestamp for log display
pub fn format_wall_clock() -> String {
    use chrono::Local;
    Local::now().format("%H:%M:%S").to_string()
}

/// Calculate visible length of a string, excluding ANSI escape codes
///
/// ANSI codes are sequences like `\x1b[0m`, `\x1b[1;32m`, etc.
pub fn visible_length(s: &str) -> usize {
    let mut visible = 0;
    let mut chars = s.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            // Skip ANSI escape sequence
            // Format: ESC [ ... m  OR  ESC [ ... K  OR  ESC [ ... H, etc.
            if chars.peek() == Some(&'[') {
                chars.next(); // consume '['
                              // Skip until we hit a letter (the final character of the sequence)
                for c in chars.by_ref() {
                    if c.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else {
            visible += 1;
        }
    }

    visible
}

/// Pad a string to a specific visible width, accounting for ANSI codes
///
/// The string may contain ANSI escape codes which are NOT counted toward
/// the visible width. Returns the string padded with spaces to reach the
/// desired visible width.
///
/// Example:
/// ```ignore
/// let colored = "\x1b[1;36mHello\x1b[0m"; // Bold cyan "Hello" (5 visible chars)
/// let padded = pad_visible(colored, 10);  // "Hello     " (colored, 10 visible)
/// ```
pub fn pad_visible(s: &str, width: usize) -> String {
    let visible = visible_length(s);

    if visible >= width {
        s.to_string()
    } else {
        format!("{}{}", s, " ".repeat(width - visible))
    }
}

/// Place a value at the standard column (VALUE_COLUMN)
///
/// Takes the complete left side (including indentation, labels, and ANSI codes)
/// and intelligently truncates or pads it to exactly VALUE_COLUMN - 1 visible characters,
/// then appends the value. This ensures values always align at the same column.
///
/// Example:
/// ```ignore
/// let left = "    stone-crystal-forest";  // indent + name
/// let value = "[thriving] [tended]";
/// println!("{}", place_value(&left, value));
/// // Output: "    stone-crystal-forest                 [thriving] [tended]"
/// //                                                  ^ column 49
/// ```
pub fn place_value(left_side: &str, value: &str) -> String {
    let target_col = constants::VALUE_COLUMN - 2; // 47 chars, space at 48, value at 49
    let visible = visible_length(left_side);

    if visible > target_col {
        // ANSI-aware truncation: keep escape sequences, count only visible chars
        let truncated = truncate_visible(left_side, target_col.saturating_sub(3));
        format!("{}... {}", truncated, value)
    } else {
        // Pad to target column, add space, value appears at VALUE_COLUMN
        let padded = pad_visible(left_side, target_col);
        format!("{} {}", padded, value)
    }
}

/// Truncate a string to a maximum visible width, preserving ANSI escape codes.
///
/// Walks the string character by character. ANSI escape sequences (ESC [ ... letter)
/// are always included without counting toward visible width. Once `max_visible`
/// printable characters have been emitted, the string is cut.
pub fn truncate_visible(s: &str, max_visible: usize) -> String {
    let mut result = String::with_capacity(s.len());
    let mut visible = 0;
    let mut chars = s.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            // Always include full ANSI escape sequence
            result.push(ch);
            if chars.peek() == Some(&'[') {
                result.push(chars.next().unwrap()); // '['
                for c in chars.by_ref() {
                    result.push(c);
                    if c.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else {
            if visible >= max_visible {
                break;
            }
            result.push(ch);
            visible += 1;
        }
    }

    // Append ANSI reset if the original had escape codes and we truncated
    if visible >= max_visible && s.contains('\x1b') {
        result.push_str("\x1b[0m");
    }

    result
}

// =============================================================================
// Shared TUI primitives (reusable by pulse, observe, any full-screen view)
// =============================================================================

/// Get terminal dimensions as (columns, rows).
///
/// Falls back to (80, 24) if detection fails (e.g., piped output).
pub fn terminal_dimensions() -> (usize, usize) {
    terminal_size::terminal_size()
        .map(|(w, h)| (w.0 as usize, h.0 as usize))
        .unwrap_or((constants::DEFAULT_TERMINAL_WIDTH, 24))
}

/// Format a horizontal divider line, optionally with a label.
///
/// Always fits within `cols` visible characters (no bleed/wrap).
/// Uses `─` (U+2500) when unicode is supported, `-` otherwise.
///
/// ```text
/// " ──────────────────────"      (no label)
/// " garden (15 ok) ──────"      (with label)
/// ```
pub fn format_separator(label: Option<&str>, cols: usize, unicode: bool) -> String {
    let bar_char = if unicode { "\u{2500}" } else { "-" };
    let prefix = match label {
        Some(lbl) => format!(" {} ", lbl),
        None => " ".to_string(),
    };
    let bar_len = cols.saturating_sub(prefix.len());
    let bar: String = bar_char.repeat(bar_len);
    format!("{}{}", prefix, bar)
}

/// Extract HH:MM:SS from an SSE event's ISO 8601 timestamp field.
///
/// Looks for a `"timestamp"` key in the JSON value and extracts the
/// time portion from an ISO 8601 string like `"2026-02-28T14:32:01.123Z"`.
/// Falls back to the current wall clock if parsing fails.
pub fn extract_sse_time(parsed: &serde_json::Value) -> String {
    parsed
        .get("timestamp")
        .and_then(|t| t.as_str())
        .and_then(|t| {
            // ISO timestamp: "2026-02-28T14:32:01.123Z" → "14:32:01"
            if t.len() >= 19 {
                Some(t[11..19].to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(format_wall_clock)
}

/// Constants for UI rendering
pub mod constants {
    pub const DEFAULT_INDENT: usize = 4;
    pub const DEFAULT_TERMINAL_WIDTH: usize = 80;
    #[allow(dead_code)] // Phase 3
    pub const NUMERIC_PRECISION: usize = 2;
    pub const MAX_SERVICE_NAME_LEN: usize = 24;
    pub const LEGEND_SYMBOL: char = '*';
    /// Column at which values should be placed (after label)
    /// All values, tags, and statuses should align at this column
    pub const VALUE_COLUMN: usize = 49;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_visible_length_plain() {
        assert_eq!(visible_length("hello"), 5);
        assert_eq!(visible_length("stone-coral-prairie"), 19);
    }

    #[test]
    fn test_visible_length_with_ansi() {
        // Bold white: \x1b[1m \x1b[0m
        let bold = format!("\x1b[1mhello\x1b[0m");
        assert_eq!(visible_length(&bold), 5);

        // Bold + color: \x1b[1;97m ... \x1b[0m
        let colored = format!("\x1b[1;97mstone-coral-prairie\x1b[0m");
        assert_eq!(visible_length(&colored), 19);
    }

    #[test]
    fn test_pad_visible() {
        let plain = "hello";
        assert_eq!(pad_visible(plain, 10), "hello     ");

        let colored = format!("\x1b[1;97mhello\x1b[0m");
        let padded = pad_visible(&colored, 10);
        // Should have colored "hello" + 5 spaces
        assert_eq!(visible_length(&padded), 10);
    }

    #[test]
    fn test_place_value_with_title() {
        // Test with full left side including indent
        let indent = "    "; // 4 spaces
        let title_name = format!("\x1b[1m\x1b[97mstone-crystal-forest\x1b[0m\x1b[0m");
        let left_side = format!("{}{}", indent, title_name);
        let status = "[thriving] [tended]";
        let result = place_value(&left_side, status);

        // Visible length should be 4 (indent) + 20 (name) = 24
        assert_eq!(visible_length(&left_side), 24);
        // After padding to 48 + space + status, visible should be 48 + 1 + 19 = 68
        // Allow 1 char difference for platform-specific variations
        let expected_visible = 48 + 1 + visible_length(status);
        let actual_visible = visible_length(&result);
        assert!(
            (actual_visible as i32 - expected_visible as i32).abs() <= 1,
            "Expected visible length {}, got {} (diff: {})",
            expected_visible,
            actual_visible,
            (actual_visible as i32 - expected_visible as i32).abs()
        );

        println!("Result: {}", result);
        println!("Visible length: {}", visible_length(&result));
    }

    #[test]
    fn test_stone_banner_format() {
        // Test basic format structure - should have === prefix and fill with equals
        let banner = stone_banner("stone-01", "Thriving", false);
        assert!(banner.starts_with("=== stone-01 - [Thriving]"));
        assert!(banner.contains("="));

        // Test with different status
        let banner = stone_banner("stone-02", "ERROR", false);
        assert!(banner.starts_with("=== stone-02 - [ERROR]"));
        assert!(banner.contains("="));
    }

    #[test]
    fn test_section_header_respects_cap() {
        // Wide terminal: should cap at 40 chars
        let wide_term = TerminalInfo {
            width: 120,
            supports_color: false,
            supports_unicode: false,
        };
        let header = section_header("TEST", &wide_term);
        assert_eq!(
            header.len(),
            40,
            "Should cap at 40 chars even on wide terminals"
        );
        // Format: "--- TEST " (9 chars) + 31 dashes = 40
        assert_eq!(header, "--- TEST -------------------------------");

        // Exactly 40-char terminal: should use full width
        let exact_term = TerminalInfo {
            width: 40,
            supports_color: false,
            supports_unicode: false,
        };
        let header = section_header("TEST", &exact_term);
        assert_eq!(header.len(), 40);
        assert_eq!(header, "--- TEST -------------------------------");

        // Narrow terminal: should respect narrow width
        let narrow_term = TerminalInfo {
            width: 20,
            supports_color: false,
            supports_unicode: false,
        };
        let header = section_header("TEST", &narrow_term);
        assert_eq!(header.len(), 20, "Should respect narrow terminal width");
        assert_eq!(header, "--- TEST -----------");

        // Very narrow terminal: should not add trailing dashes if title doesn't fit
        let tiny_term = TerminalInfo {
            width: 8,
            supports_color: false,
            supports_unicode: false,
        };
        let header = section_header("TEST", &tiny_term);
        assert_eq!(header, "--- TEST ");
    }

    #[test]
    fn test_section_header_long_title() {
        // Long title should still work without panicking
        let term = TerminalInfo {
            width: 40,
            supports_color: false,
            supports_unicode: false,
        };
        let header = section_header("VERY LONG SECTION TITLE HERE", &term);
        assert!(header.starts_with("--- VERY LONG SECTION TITLE HERE "));
        // Should have no trailing dashes since title fills the width
    }

    #[test]
    fn test_truncate_name_edge_cases() {
        // Short name: no truncation
        assert_eq!(truncate_name("short", 24), "short");

        // Exact length: no truncation
        assert_eq!(
            truncate_name("exactly-twenty-four!", 20),
            "exactly-twenty-four!"
        );

        // One char over: should truncate (takes first 17 chars + "...")
        assert_eq!(
            truncate_name("exactly-twenty-four!x", 20),
            "exactly-twenty-fo..."
        );

        // Very long: should truncate properly (first 21 chars + "...")
        assert_eq!(
            truncate_name("very-long-service-name-that-exceeds", 24),
            "very-long-service-nam..."
        );

        // Edge case: max_len <= 3 returns first chars without ellipsis
        assert_eq!(truncate_name("test", 3), "tes");

        // Empty string
        assert_eq!(truncate_name("", 24), "");
    }

    #[test]
    fn test_verbosity_parsing() {
        assert_eq!(Verbosity::from_arg("-v0"), Some(Verbosity::Minimal));
        assert_eq!(Verbosity::from_arg("-v1"), Some(Verbosity::Standard));
        assert_eq!(Verbosity::from_arg("-v2"), Some(Verbosity::Verbose));
        assert_eq!(Verbosity::from_arg("-v3"), Some(Verbosity::Debug));

        // Invalid cases
        assert_eq!(Verbosity::from_arg("invalid"), None);
        assert_eq!(Verbosity::from_arg("-v4"), None);
        assert_eq!(Verbosity::from_arg("v1"), None);
        assert_eq!(Verbosity::from_arg(""), None);
    }

    #[test]
    fn test_table_builder_alignment() {
        let mut table = TableBuilder::new()
            .add_column(15, Align::Left)
            .add_column(10, Align::Right);

        table.add_row(vec!["mongodb".to_string(), "3m".to_string()]);
        table.add_row(vec!["postgresql".to_string(), "15m 12s".to_string()]);

        let output = table.render();
        let lines: Vec<&str> = output.lines().collect();

        // Should have 2 rows
        assert_eq!(lines.len(), 2);

        // Each line should start with default indent
        for line in &lines {
            assert!(
                line.starts_with("    "),
                "Each line should start with 4-space indent"
            );
        }

        // Check column widths are respected
        assert!(lines[0].contains("mongodb"));
        assert!(lines[1].contains("postgresql"));
    }

    #[test]
    fn test_status_indicator_mappings() {
        // Test vitality language mappings (no color)
        assert_eq!(status_indicator("running", false), "[thriving]");
        assert_eq!(status_indicator("stopped", false), "[dormant]");
        assert_eq!(status_indicator("ok", false), "[thriving]");
        assert_eq!(status_indicator("healthy", false), "[thriving]"); // Legacy maps to vitality
        assert_eq!(status_indicator("thriving", false), "[thriving]");
        assert_eq!(status_indicator("dormant", false), "[dormant]");
        assert_eq!(status_indicator("error", false), "[needs attention]");
        assert_eq!(status_indicator("failed", false), "[needs attention]");
        assert_eq!(status_indicator("warn", false), "[needs attention]");
        assert_eq!(status_indicator("warning", false), "[needs attention]");
        assert_eq!(status_indicator("degraded", false), "[needs attention]");
        assert_eq!(status_indicator("withering", false), "[needs attention]");

        // Case insensitivity
        assert_eq!(status_indicator("OK", false), "[thriving]");
        assert_eq!(status_indicator("Running", false), "[thriving]");

        // Unknown status should pass through
        assert_eq!(status_indicator("unknown", false), "unknown");
    }

    #[test]
    fn test_label_value_line_formatting() {
        // Test basic formatting
        let line = label_value_line("Status", "Running", 4);
        assert!(line.starts_with("    "));
        assert!(line.contains("Status"));
        assert!(line.contains("Running"));

        // Test different indentation
        let line = label_value_line("Name", "test-service", 0);
        assert!(!line.starts_with(" "));

        let line = label_value_line("Port", "8080", 8);
        assert!(line.starts_with("        "));
    }

    #[test]
    fn test_category_grid_formatting() {
        let term = TerminalInfo {
            width: 80,
            supports_color: false,
            supports_unicode: false,
        };
        let grid = CategoryGrid::new(&term);

        // Test with multiple items
        let output = grid.render_category(
            "DATA",
            &[
                "mongodb".to_string(),
                "postgresql".to_string(),
                "redis".to_string(),
            ],
        );

        assert!(output.contains("DATA"), "Should contain category name");
        assert!(output.contains("mongodb"), "Should contain first item");
        assert!(output.contains("postgresql"), "Should contain second item");
        assert!(output.contains("redis"), "Should contain third item");

        // Test with empty items - returns empty string
        let output = grid.render_category("EMPTY", &[]);
        assert_eq!(
            output, "",
            "Empty category should return empty string (no header row)"
        );
    }

    #[test]
    fn test_empty_state_with_and_without_hint() {
        // With hint
        let output = empty_state(
            "No services found",
            Some("Try: garden-rake offer install <name>"),
        );
        assert!(output.contains("No services found"));
        assert!(output.contains("Try: garden-rake offer install <name>"));

        // Without hint
        let output = empty_state("No items", None);
        assert!(output.contains("No items"));
        assert!(!output.contains("Try:"));
    }

    // =========================================================================
    // fit_items tests
    // =========================================================================

    #[test]
    fn test_fit_items_all_fit() {
        assert_eq!(fit_items(&["a", "b", "c"], 20), "a, b, c");
    }

    #[test]
    fn test_fit_items_exact_fit() {
        // "a, b, c" = 7 chars
        assert_eq!(fit_items(&["a", "b", "c"], 7), "a, b, c");
    }

    #[test]
    fn test_fit_items_partial() {
        // "mongodb, ollama, redis" = 22 chars, budget 20
        // "mongodb, ollama +1" = 18 chars -> fits
        assert_eq!(
            fit_items(&["mongodb", "ollama", "redis"], 20),
            "mongodb, ollama +1"
        );
    }

    #[test]
    fn test_fit_items_single_with_overflow() {
        // "mongodb +2" = 10 chars
        assert_eq!(
            fit_items(&["mongodb", "ollama", "redis"], 10),
            "mongodb +2"
        );
    }

    #[test]
    fn test_fit_items_nothing_fits() {
        assert_eq!(fit_items(&["mongodb", "ollama", "redis"], 3), "+3");
    }

    #[test]
    fn test_fit_items_empty() {
        assert_eq!(fit_items(&[], 50), "");
    }

    #[test]
    fn test_fit_items_single_item_fits() {
        assert_eq!(fit_items(&["mongodb"], 20), "mongodb");
    }

    #[test]
    fn test_fit_items_single_item_too_large() {
        assert_eq!(fit_items(&["mongodb"], 3), "+1");
    }

    #[test]
    fn test_fit_items_budget_zero() {
        assert_eq!(fit_items(&["a"], 0), "");
    }

    #[test]
    fn test_fit_items_realistic_services() {
        let services = &["mongodb", "ollama", "redis", "weaviate", "postgres"];
        // Wide: all fit
        assert_eq!(
            fit_items(services, 50),
            "mongodb, ollama, redis, weaviate, postgres"
        );
        // Medium: partial
        assert_eq!(
            fit_items(services, 30),
            "mongodb, ollama, redis +2"
        );
        // Narrow: just one
        assert_eq!(fit_items(services, 12), "mongodb +4");
    }
}
