//! Console event types and structures

/// Event categories for structured console output
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventCategory {
    System,
    Config,
    Manifests,
    Offerings,
    Services,
    Jobs,
    Storage,
    Network,
    Docker,
    Discovery,
    Health,
    API,
    Security,
    Ops,
    Cluster,
}

impl EventCategory {
    /// Get the padded display name (9 characters)
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::System => "System   ",
            Self::Config => "Config   ",
            Self::Manifests => "Manifests",
            Self::Offerings => "Offerings",
            Self::Services => "Services ",
            Self::Jobs => "Jobs     ",
            Self::Storage => "Storage  ",
            Self::Network => "Network  ",
            Self::Docker => "Docker   ",
            Self::Discovery => "Discovery",
            Self::Health => "Health   ",
            Self::API => "API      ",
            Self::Security => "Security ",
            Self::Ops => "Ops      ",
            Self::Cluster => "Cluster  ",
        }
    }
    
    /// Get color hint for this category (DRY - single source of truth)
    pub fn color_hint(&self) -> AnsiColor {
        match self {
            Self::System => AnsiColor::Cyan,
            Self::Config => AnsiColor::Blue,
            Self::Manifests => AnsiColor::Magenta,
            Self::Offerings => AnsiColor::Magenta,
            Self::Services => AnsiColor::Green,
            Self::Jobs => AnsiColor::Yellow,
            Self::Storage => AnsiColor::Blue,
            Self::Network => AnsiColor::Cyan,
            Self::Docker => AnsiColor::Blue,
            Self::Discovery => AnsiColor::Cyan,
            Self::Health => AnsiColor::Green,
            Self::API => AnsiColor::White,
            Self::Security => AnsiColor::Red,
            Self::Ops => AnsiColor::Yellow,
            Self::Cluster => AnsiColor::Magenta,
        }
    }
}

/// Event status for structured console output
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventStatus {
    // System
    Starting, Ready, Shutting, Stopped, FirstBoot, FirstBootDone, FsReady, FsError, FsPending,
    FsWritable, FsRemount, SignalReceived, AdminShutdown, Draining, PreinstallComplete,
    HttpError, Connected,
    
    // Config
    Reading, Loaded, Merged, ParseError, ReadError, NotFound, PreinstallFound, PreinstallLoaded,
    PreinstallError,
    
    // Manifests
    Scanning, Found, Loading, Parsed, Validated, CompatRules, Refresh, Updated, Invalid,
    DirFound, DirMissing, TryingCompose, CompatError, NoCompat,
    
    // Offerings
    Building, Built, BuildError, Planting, RebuildError,
    
    // Services (reuse Starting/Stopped from System)
    Requesting, Pulling, Creating, Running, Healthy, Stopping, Removing, Removed,
    Upgrading, Upgraded, Adopting, OrphanFound, NoManifest, AdoptError, ScanComplete, ScanError,
    ListError, CleanupError, StopError, StartError, UpgradeError,
    
    // Jobs (reuse Retry from Docker, CompatError from Manifests)
    Queued, Started, Progress, Completed, Failed, Cancelled, PreinstallDone,
    
    // Storage
    Saving, SaveError, LoadError, DeleteError, WriteError, MkdirError, ChmodError, MoveError,
    
    // Network
    Listening, Binding, BindError,
    
    // Docker
    Disconnected, PullProgress, PullComplete, Retry, ImagePull,
    
    // Discovery
    Request, Response, MdnsActive, MdnsError, UdpError, LanternReg, LanternUnreachable,
    LanternError, LanternFatal,
    
    // Health
    Ok, Degraded, Unhealthy,
    
    // API
    Post, Get, Put, Delete, SseLag, LogStreamError,
    
    // Security
    KeystoneGen, KeystoneLoad, KeystoneExp, AuthEnable, AuthDisable, AuthSuccess, AuthDenied,
    AuthExpired, RateLimited, StoneTrust, StoneReject, TlsEnabled,
    
    // Ops (remove duplicate Validated)
    Active, Cordon, DrainStart, DrainDone, Uncordon, RetireSched, RetireStart, RetireDone,
    StoneJoin, StoneLeave, RefreshReq, DecodeError, ValidationError, UnknownComponent,
    Staged, RestartReq, RestartTriggered, RestartWarning, RestartError, ShutdownReq, ShutdownDone,
    ShutdownTimeout, ShutdownError, Kill, Conflict, ForceFlag, ForceError,
    
    // Cluster  
    Joined, Left, LeaderElected, LeaderLost, Syncing, Synced,
}

impl EventStatus {
    /// Get the padded display name (14 characters)
    pub fn display_name(&self) -> &'static str {
        match self {
            // System
            Self::Starting => "STARTING      ",
            Self::Ready => "READY         ",
            Self::Shutting => "SHUTTING_DOWN ",
            Self::Stopped => "STOPPED       ",
            Self::FirstBoot => "FIRST_BOOT    ",
            Self::FirstBootDone => "FIRST_BOOT_DONE",
            Self::FsReady => "FS_READY      ",
            Self::FsError => "FS_ERROR      ",
            Self::FsPending => "FS_PENDING    ",
            Self::FsWritable => "FS_WRITABLE   ",
            Self::FsRemount => "FS_REMOUNT    ",
            Self::SignalReceived => "SIGNAL_RCVD   ",
            Self::AdminShutdown => "ADMIN_SHUTDOWN",
            Self::Draining => "DRAINING      ",
            Self::PreinstallComplete => "PREINSTALL_OK ",
            Self::HttpError => "HTTP_ERROR    ",
            Self::Connected => "CONNECTED     ",
            
            // Config
            Self::Reading => "READING       ",
            Self::Loaded => "LOADED        ",
            Self::Merged => "MERGED        ",
            Self::ParseError => "PARSE_ERROR   ",
            Self::ReadError => "READ_ERROR    ",
            Self::NotFound => "NOT_FOUND     ",
            Self::PreinstallFound => "PREINSTALL_FOUND",
            Self::PreinstallLoaded => "PREINSTALL_LOADED",
            Self::PreinstallError => "PREINSTALL_ERR",
            
            // Manifests
            Self::Scanning => "SCANNING      ",
            Self::Found => "FOUND         ",
            Self::Loading => "LOADING       ",
            Self::Parsed => "PARSED        ",
            Self::Validated => "VALIDATED     ",
            Self::CompatRules => "COMPAT_RULES  ",
            Self::Refresh => "REFRESH       ",
            Self::Updated => "UPDATED       ",
            Self::Invalid => "INVALID       ",
            Self::DirFound => "DIR_FOUND     ",
            Self::DirMissing => "DIR_MISSING   ",
            Self::TryingCompose => "TRYING_COMPOSE",
            Self::CompatError => "COMPAT_ERROR  ",
            Self::NoCompat => "NO_COMPAT     ",
            
            // Offerings
            Self::Building => "BUILDING      ",
            Self::Built => "BUILT         ",
            Self::BuildError => "BUILD_ERROR   ",
            Self::Planting => "PLANTING      ",
            Self::RebuildError => "REBUILD_ERROR ",
            
            // Services (Starting/Stopped are in System, DeleteError in Storage)
            Self::Requesting => "REQUESTING    ",
            Self::Pulling => "PULLING       ",
            Self::Creating => "CREATING      ",
            Self::Running => "RUNNING       ",
            Self::Healthy => "HEALTHY       ",
            Self::Stopping => "STOPPING      ",
            Self::Removing => "REMOVING      ",
            Self::Removed => "REMOVED       ",
            Self::Upgrading => "UPGRADING     ",
            Self::Upgraded => "UPGRADED      ",
            Self::Adopting => "ADOPTING      ",
            Self::OrphanFound => "ORPHAN_FOUND  ",
            Self::NoManifest => "NO_MANIFEST   ",
            Self::AdoptError => "ADOPT_ERROR   ",
            Self::ScanComplete => "SCAN_COMPLETE ",
            Self::ScanError => "SCAN_ERROR    ",
            Self::ListError => "LIST_ERROR    ",
            Self::CleanupError => "CLEANUP_ERROR ",
            Self::StopError => "STOP_ERROR    ",
            Self::StartError => "START_ERROR   ",
            Self::UpgradeError => "UPGRADE_ERROR ",
            
            // Jobs (Retry in Docker, CompatError in Manifests)
            Self::Queued => "QUEUED        ",
            Self::Started => "STARTED       ",
            Self::Progress => "PROGRESS      ",
            Self::Completed => "COMPLETED     ",
            Self::Failed => "FAILED        ",
            Self::Cancelled => "CANCELLED     ",
            Self::PreinstallDone => "PREINSTALL_DONE",
            
            // Storage
            Self::Saving => "SAVING        ",
            Self::SaveError => "SAVE_ERROR    ",
            Self::LoadError => "LOAD_ERROR    ",
            Self::DeleteError => "DELETE_ERROR  ",
            Self::WriteError => "WRITE_ERROR   ",
            Self::MkdirError => "MKDIR_ERROR   ",
            Self::ChmodError => "CHMOD_ERROR   ",
            Self::MoveError => "MOVE_ERROR    ",
            
            // Network
            Self::Listening => "LISTENING     ",
            Self::Binding => "BINDING       ",
            Self::BindError => "BIND_ERROR    ",
            
            // Docker (Connected is in System)
            Self::Disconnected => "DISCONNECTED  ",
            Self::PullProgress => "PULL_PROGRESS ",
            Self::PullComplete => "PULL_COMPLETE ",
            Self::Retry => "RETRY         ",
            Self::ImagePull => "IMAGE_PULL    ",
            
            // Discovery
            Self::Request => "REQUEST       ",
            Self::Response => "RESPONSE      ",
            Self::MdnsActive => "MDNS_ACTIVE   ",
            Self::MdnsError => "MDNS_ERROR    ",
            Self::UdpError => "UDP_ERROR     ",
            Self::LanternReg => "LANTERN_REG   ",
            Self::LanternUnreachable => "LANTERN_UNREACH",
            Self::LanternError => "LANTERN_ERROR ",
            Self::LanternFatal => "LANTERN_FATAL ",
            
            // Health
            Self::Ok => "OK            ",
            Self::Degraded => "DEGRADED      ",
            Self::Unhealthy => "UNHEALTHY     ",
            
            // API
            Self::Post => "POST          ",
            Self::Get => "GET           ",
            Self::Put => "PUT           ",
            Self::Delete => "DELETE        ",
            Self::SseLag => "SSE_LAG       ",
            Self::LogStreamError => "LOG_STREAM_ERR",
            
            // Security
            Self::KeystoneGen => "KEYSTONE_GEN  ",
            Self::KeystoneLoad => "KEYSTONE_LOAD ",
            Self::KeystoneExp => "KEYSTONE_EXP  ",
            Self::AuthEnable => "AUTH_ENABLE   ",
            Self::AuthDisable => "AUTH_DISABLE  ",
            Self::AuthSuccess => "AUTH_SUCCESS  ",
            Self::AuthDenied => "AUTH_DENIED   ",
            Self::AuthExpired => "AUTH_EXPIRED  ",
            Self::RateLimited => "RATE_LIMITED  ",
            Self::StoneTrust => "STONE_TRUST   ",
            Self::StoneReject => "STONE_REJECT  ",
            Self::TlsEnabled => "TLS_ENABLED   ",
            
            // Ops
            Self::Active => "ACTIVE        ",
            Self::Cordon => "CORDON        ",
            Self::DrainStart => "DRAIN_START   ",
            Self::DrainDone => "DRAIN_DONE    ",
            Self::Uncordon => "UNCORDON      ",
            Self::RetireSched => "RETIRE_SCHED  ",
            Self::RetireStart => "RETIRE_START  ",
            Self::RetireDone => "RETIRE_DONE   ",
            Self::StoneJoin => "STONE_JOIN    ",
            Self::StoneLeave => "STONE_LEAVE   ",
            Self::RefreshReq => "REFRESH_REQ   ",
            Self::DecodeError => "DECODE_ERROR  ",
            Self::ValidationError => "VALIDATION_ERR",
            Self::UnknownComponent => "UNKNOWN_COMP  ",
            Self::Staged => "STAGED        ",
            Self::RestartReq => "RESTART_REQ   ",
            Self::RestartTriggered => "RESTART_TRIG  ",
            Self::RestartWarning => "RESTART_WARN  ",
            Self::RestartError => "RESTART_ERROR ",
            Self::ShutdownReq => "SHUTDOWN_REQ  ",
            Self::ShutdownDone => "SHUTDOWN_DONE ",
            Self::ShutdownTimeout => "SHUTDOWN_TMOUT",
            Self::ShutdownError => "SHUTDOWN_ERROR",
            Self::Kill => "KILL          ",
            Self::Conflict => "CONFLICT      ",
            Self::ForceFlag => "FORCE_FLAG    ",
            Self::ForceError => "FORCE_ERROR   ",
            
            // Cluster
            Self::Joined => "JOINED        ",
            Self::Left => "LEFT          ",
            Self::LeaderElected => "LEADER_ELECTED",
            Self::LeaderLost => "LEADER_LOST   ",
            Self::Syncing => "SYNCING       ",
            Self::Synced => "SYNCED        ",
        }
    }
    
    /// Determine if this status represents an error/failure
    pub fn is_error(&self) -> bool {
        matches!(self,
            Self::ParseError | Self::ReadError | Self::BuildError | Self::RebuildError |
            Self::Failed | Self::SaveError | Self::LoadError | Self::DeleteError |
            Self::WriteError | Self::MkdirError | Self::ChmodError | Self::MoveError |
            Self::BindError | Self::MdnsError | Self::UdpError | Self::LanternError |
            Self::LanternFatal | Self::Unhealthy | Self::LogStreamError | Self::DecodeError |
            Self::ValidationError | Self::RestartError | Self::ShutdownError | Self::ShutdownTimeout |
            Self::FsError | Self::HttpError | Self::CleanupError | Self::StopError |
            Self::StartError | Self::UpgradeError | Self::ScanError |
            Self::ListError | Self::AdoptError | Self::CompatError | Self::PreinstallError |
            Self::Invalid | Self::NoManifest | Self::DirMissing |
            Self::ForceError
        )
    }
    
    /// Determine if this status represents a warning
    pub fn is_warning(&self) -> bool {
        matches!(self,
            Self::Retry | Self::Degraded | Self::RestartWarning | Self::FsPending |
            Self::LanternUnreachable | Self::OrphanFound | Self::Conflict | Self::ForceFlag
        )
    }
    
    /// Determine if this status represents success/completion
    pub fn is_success(&self) -> bool {
        matches!(self,
            Self::Ready | Self::Completed | Self::Loaded | Self::Built | Self::Validated |
            Self::Healthy | Self::Connected | Self::Upgraded | Self::FirstBootDone |
            Self::FsReady | Self::PreinstallComplete | Self::ShutdownDone | Self::Synced |
            Self::PullComplete | Self::Ok | Self::Joined | Self::LeaderElected |
            Self::FsWritable | Self::PreinstallDone
        )
    }
    
    /// Get severity hint for this status (DRY - single source of truth for semantic level)
    pub fn severity_hint(&self) -> Severity {
        if self.is_error() {
            Severity::Error
        } else if self.is_warning() {
            Severity::Warning
        } else if self.is_success() {
            Severity::Info
        } else {
            Severity::Debug // In-progress, reading, scanning, etc.
        }
    }
}

/// A structured console event
#[derive(Debug, Clone)]
pub struct ConsoleEvent {
    pub category: EventCategory,
    pub status: EventStatus,
    pub message: String,
    pub hint: Option<FormatHint>,
}

impl ConsoleEvent {
    /// Create a new console event
    pub fn new(category: EventCategory, status: EventStatus, message: impl Into<String>) -> Self {
        Self {
            category,
            status,
            message: message.into(),
            hint: None,
        }
    }
    
    /// Create a new event with format hint
    pub fn with_hint(
        category: EventCategory,
        status: EventStatus,
        message: impl Into<String>,
        hint: FormatHint,
    ) -> Self {
        Self {
            category,
            status,
            message: message.into(),
            hint: Some(hint),
        }
    }
    
    /// Helper to determine if this event should be logged (useful for deduplication checks)
    pub fn dedupe_key(&self) -> String {
        format!("{:?}::{:?}::{}", self.category, self.status, self.message)
    }
}

/// Optional formatting hints for custom rendering
#[derive(Debug, Clone)]
pub struct FormatHint {
    pub color: Option<AnsiColor>,
    pub severity: Option<Severity>,
}

impl FormatHint {
    pub fn new() -> Self {
        Self { color: None, severity: None }
    }
    
    pub fn with_color(mut self, color: AnsiColor) -> Self {
        self.color = Some(color);
        self
    }
    
    pub fn with_severity(mut self, severity: Severity) -> Self {
        self.severity = Some(severity);
        self
    }
}

impl Default for FormatHint {
    fn default() -> Self {
        Self::new()
    }
}

/// ANSI color codes for hints
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnsiColor {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    BrightBlack,
    BrightRed,
    BrightGreen,
    BrightYellow,
    BrightBlue,
    BrightMagenta,
    BrightCyan,
    BrightWhite,
}

impl AnsiColor {
    /// Get ANSI escape code for this color
    pub fn code(&self) -> &'static str {
        match self {
            Self::Black => "\x1b[30m",
            Self::Red => "\x1b[31m",
            Self::Green => "\x1b[32m",
            Self::Yellow => "\x1b[33m",
            Self::Blue => "\x1b[34m",
            Self::Magenta => "\x1b[35m",
            Self::Cyan => "\x1b[36m",
            Self::White => "\x1b[37m",
            Self::BrightBlack => "\x1b[90m",
            Self::BrightRed => "\x1b[91m",
            Self::BrightGreen => "\x1b[92m",
            Self::BrightYellow => "\x1b[93m",
            Self::BrightBlue => "\x1b[94m",
            Self::BrightMagenta => "\x1b[95m",
            Self::BrightCyan => "\x1b[96m",
            Self::BrightWhite => "\x1b[97m",
        }
    }
}

/// Severity levels for events
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Debug,
    Info,
    Warning,
    Error,
}

impl Severity {
    /// Get ANSI color for this severity level
    pub fn color(&self) -> AnsiColor {
        match self {
            Self::Debug => AnsiColor::BrightBlack,
            Self::Info => AnsiColor::Green,
            Self::Warning => AnsiColor::Yellow,
            Self::Error => AnsiColor::Red,
        }
    }
}
