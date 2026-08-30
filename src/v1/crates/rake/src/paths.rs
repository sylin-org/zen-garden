    pub const OFFERINGS: &str = "/api/v1/offerings";

    /// Local storage (L22): banks and adoptable volumes.
    pub const STORAGE: &str = "/api/v1/storage";
    /// The adopt ceremony's face.
    pub const STORAGE_ADOPT: &str = "/api/v1/storage/adopt";
    /// The room's banks (grid law, ADR-0004 §4).
    pub const STORAGE_GARDEN: &str = "/api/v1/garden/storage";
    /// The eject verb's face.
    pub fn storage_eject(bank: &str) -> String {
        format!("{STORAGE}/{}/eject", encode_segment(bank))
    }

    /// The roles declaration's face.
    pub fn storage_roles(bank: &str) -> String {
        format!("{STORAGE}/{}/roles", encode_segment(bank))
    }

    /// The restore-rehearsal face (J2's proof loop).
    pub fn rehearse(name: &str) -> String {
        format!("{OFFERINGS}/{}/rehearse", encode_segment(name))
    }

    /// The capabilities face (W1): what an offering holds.
    pub fn capabilities(name: &str) -> String {
        format!("{OFFERINGS}/{}/capabilities", encode_segment(name))
    }

    /// The offering-logs stream face. `tail` and `timestamps` ride as
    /// query pairs.
    pub fn logs_stream(name: &str, tail: Option<u64>, timestamps: bool) -> String {
        let mut params = Vec::new();
        if let Some(n) = tail {
            params.push(format!("tail={n}"));
        }
        if timestamps {
            params.push("timestamps=true".into());
        }
        let query = if params.is_empty() {
            String::new()
        } else {
            format!("?{}", params.join("&"))
        };
        format!("{OFFERINGS}/{}/logs/stream{query}", encode_segment(name))
    }

    /// The bank-files list face. `path` and `depth` ride as query pairs.
    pub fn storage_files(bank: &str, path: Option<&str>, depth: Option<&str>) -> String {
        let mut target = format!("{STORAGE}/{}/files", encode_segment(bank));
        let mut params = Vec::new();
        if let Some(dir) = path {
            params.push(format!("path={}", encode_segment(dir)));
        }
        if let Some(d) = depth {
            params.push(format!("depth={}", encode_segment(d)));
        }
        if !params.is_empty() {
            target.push('?');
            target.push_str(&params.join("&"));
        }
        target
    }

    /// One file's face on a bank. The path is wire-encoded whole — `/`
    /// inside a name nests on the bank, everything unsafe escapes.
    pub fn storage_file(bank: &str, path: &str) -> String {
        format!(
            "{STORAGE}/{}/files/{}",
            encode_segment(bank),
            encode_segment(path)
        )
    }

    /// Percent-encode one wire path segment: everything outside the
    /// unreserved set escapes, so names with spaces, `#` or `/` ride
    /// correctly. Zero deps (P5) — one table of safe bytes.
    pub fn encode_segment(s: &str) -> String {
        const HEX: &[u8; 16] = b"0123456789ABCDEF";
        let mut out = String::with_capacity(s.len());
        for b in s.bytes() {
            match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                    out.push(b as char)
                }
                other => {
                    out.push('%');
                    out.push(HEX[usize::from(other >> 4)] as char);
                    out.push(HEX[usize::from(other & 15)] as char);
                }
            }
        }
        out
    }

    /// The living will's faces (ADR-0005 §2).
    pub fn capture(name: &str) -> String {
        format!("{OFFERINGS}/{name}/capture")
    }

    /// The last-run report face.
    pub fn capture_last(name: &str) -> String {
        format!("{OFFERINGS}/{name}/capture")
    }

    /// The replant face.
    pub fn replant(name: &str) -> String {
        format!("{OFFERINGS}/{name}/replant")
    }

    pub fn record(name: &str) -> String {
        format!("{OFFERINGS}/{name}")
    }

    pub fn rest(name: &str) -> String {
        format!("{OFFERINGS}/{name}/rest")
    }

    pub fn wake(name: &str) -> String {
        format!("{OFFERINGS}/{name}/wake")
    }
