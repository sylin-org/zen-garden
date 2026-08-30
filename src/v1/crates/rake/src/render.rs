//! The answer renderers and pure response helpers: JSON in, human
//! words out. No transport, no state (L21).

use super::*;

pub fn uris_from_garden(v: &serde_json::Value) -> Result<serde_json::Value, String> {
    let mut uris = Vec::new();
    for s in envelope_plain(v)?["stones"].as_array().into_iter().flatten() {
        let ip = s["stone"]["network"]["address"]["ip"].as_str().unwrap_or("?");
        for svc in s["inventory"]["services"]["items"].as_array().into_iter().flatten() {
            let stem = svc["stem"].as_str().unwrap_or("?");
            uris.push(match svc["ports"].as_object().and_then(|m| m.values().next()) {
                Some(p) => format!("{stem}://{ip}:{p}"),
                None => format!("{stem}://{ip}"),
            });
        }
    }
    Ok(serde_json::json!(uris))
}

/// The storage faces. Every verb here is the 1:1 client of one API face:
/// list -> GET /api/v1/storage; adopt -> POST /api/v1/storage/adopt.
/// `rake watch <offering> logs`: cascade to the first moss that will
/// speak, follow the garden's redirect if the offering grows elsewhere,
/// print lines until the stream ends or `--until` matches (exit 0).

pub fn capability_satisfied(held: &str, wanted: &str) -> bool {
    held == wanted || held.strip_suffix(":latest") == Some(wanted)
}

/// The wish's selectors, as the operator typed them conceptually.

pub fn want_string(wish: &garden_contract::wish::Wish) -> String {
    wish.selectors
        .iter()
        .map(|s| format!("{}:{}", s.kind, s.item))
        .collect::<Vec<_>>()
        .join(", ")
}


pub fn service_matches(
    wish_fqn: &str,
    wish_stem: &str,
    wish_named_instance: bool,
    service_name: &str,
) -> bool {
    if service_name == wish_fqn {
        return true;
    }
    // A bare-stem wish accepts any instance of the capability; a named
    // instance wants exactly itself.
    !wish_named_instance && service_name.split("::").next() == Some(wish_stem)
}

/// The connection promise (J1) for one capability.

pub fn connection_uri(stem: &str, ip: &str, port: Option<u64>) -> String {
    match port {
        Some(p) => format!("{stem}://{ip}:{p}"),
        None => format!("{stem}://{ip}"),
    }
}

/// The gardener's update ritual (J3): check or apply. The room's mosses
/// do the checking and applying (their worlds know their images); rake is
/// the conductor — canary ordering, health between stones, halt on red.

pub fn render_bank_files(v: &serde_json::Value) -> Result<(), String> {
    let rows = v["files"].as_array();
    match rows {
        Some(r) if !r.is_empty() => {
            println!("{:<40} {:<5} {:>10}  MODIFIED", "NAME", "KIND", "SIZE");
            for e in r {
                println!(
                    "{:<40} {:<5} {:>10}  {}",
                    e["name"].as_str().unwrap_or("?"),
                    e["kind"].as_str().unwrap_or("?"),
                    e["size_bytes"].as_u64().map(human_bytes).unwrap_or_else(|| "-".into()),
                    e["modified_at"].as_str().unwrap_or("-"),
                );
            }
        }
        _ => println!("The bank holds no files here."),
    }
    Ok(())
}

/// Read the `--file` side of `storage put`: a local path, or `-` for stdin.

pub fn render_rehearsal(v: &serde_json::Value) -> Result<(), String> {
    let r = &v["rehearsal"];
    let name = display_name(r["name"].as_str().unwrap_or("?"));
    let green = r["green"] == serde_json::json!(true);
    if green {
        println!("{name} rehearsal GREEN — the checkpoint boots");
    } else {
        println!(
            "{name} rehearsal RED — {}",
            r["error"].as_str().unwrap_or("the proof failed")
        );
    }
    println!("  checkpoint  {}", r["checkpoint"].as_str().unwrap_or("-"));
    let h = r["hash"].as_str().unwrap_or("-");
    println!(
        "  restored    {} files, {} bytes, sha {}...",
        r["files"],
        r["bytes"],
        h.get(..8).unwrap_or(h)
    );
    println!(
        "  container   {} ({}s)",
        r["container_state"].as_str().unwrap_or("-"),
        r["container_ran_secs"]
    );
    if !green {
        return Err(format!(
            "{name} failed its restore rehearsal — the safety net is not safe"
        ));
    }
    Ok(())
}

/// Bytes for human eyes.

pub fn human_bytes(n: u64) -> String {
    let units = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = n as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < units.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{n} {}", units[0])
    } else {
        format!("{value:.1} {}", units[unit])
    }
}

/// The room's banks: one row per (stone, bank), self marked.

pub fn render_garden_storage(v: &serde_json::Value) -> Result<(), String> {
    let rows = v["banks"].as_array();
    match rows {
        Some(r) if !r.is_empty() => {
            println!("{:<22} {:<26} {:<10} CAPACITY", "STONE", "BANK", "STATE");
            for row in r {
                let marker = if row["self"] == true { " (me)" } else { "" };
                println!(
                    "{:<22} {:<26} {:<10} {}",
                    format!("{}{}", row["stone"].as_str().unwrap_or("?"), marker),
                    display_name(row["bank"]["fqn"].as_str().unwrap_or("?")),
                    row["bank"]["state"].as_str().unwrap_or("?"),
                    row["bank"]["capacity_bytes"]
                        .as_u64()
                        .map(human_bytes)
                        .unwrap_or_else(|| "-".into()),
                );
            }
        }
        _ => println!("The garden holds no banks yet."),
    }
    Ok(())
}

/// `rake list`: what the attached stone hosts, each with its URI —
/// `stem://host:home`. The connection promise as output (J1).

pub fn render_list(v: &serde_json::Value) -> Result<(), String> {
    let rows = v["offerings"].as_array();
    match rows {
        Some(r) if !r.is_empty() => {
            println!("{:<26} {:<10} {:<12} URI", "OFFERING", "STATUS", "HOME");
            for o in r {
                let stem = o["identity"]["stem"].as_str().unwrap_or("?");
                let home = o["mode"]["port_map"]
                    .as_object()
                    .and_then(|m| m.values().next())
                    .and_then(|p| p.as_u64())
                    .map(|p| p.to_string())
                    .unwrap_or_else(|| "-".into());
                println!(
                    "{:<26} {:<10} {:<12} {}://{}",
                    display_name(o["identity"]["name"].as_str().unwrap_or("?")),
                    o["state"]["status"].as_str().unwrap_or("?"),
                    home,
                    stem,
                    home,
                );
            }
        }
        _ => println!("Nothing planted on this stone yet. Try: rake offer <name>"),
    }
    Ok(())
}

/// The banks table: what this stone holds, and what it could adopt.

pub fn render_storage(v: &serde_json::Value) -> Result<(), String> {
    let banks = v["banks"].as_array();
    match banks {
        Some(b) if !b.is_empty() => {
            println!("{:<26} {:<10} {:<22} CAPACITY", "BANK", "STATE", "DEVICE");
            for bank in b {
                let cap = bank["capacity_bytes"]
                    .as_u64()
                    .map(human_bytes)
                    .unwrap_or_else(|| "-".into());
                println!(
                    "{:<26} {:<10} {:<22} {}",
                    display_name(bank["fqn"].as_str().unwrap_or("?")),
                    bank["state"].as_str().unwrap_or("?"),
                    bank["mount_point"].as_str().unwrap_or("?"),
                    cap,
                );
            }
        }
        _ => println!("No banks adopted yet on this stone."),
    }
    if let Some(adoptable) = v["adoptable"].as_array() {
        for vol in adoptable {
            println!(
                "ready to adopt: {} ({}) — rake storage adopt <device> --name <bank>",
                vol["device"].as_str().unwrap_or("?"),
                vol["capacity_bytes"]
                    .as_u64()
                    .map(human_bytes)
                    .unwrap_or_else(|| "unknown size".into()),
            );
        }
    }
    Ok(())
}


pub fn envelope_plain(v: &serde_json::Value) -> Result<serde_json::Value, String> {
    v.get("data")
        .cloned()
        .ok_or_else(|| "response lacked the standard 'data' envelope".to_string())
}


pub fn envelope(v: &serde_json::Value, key: &str) -> Result<serde_json::Value, String> {
    envelope_plain(v)?
        .get(key)
        .cloned()
        .ok_or_else(|| format!("response lacked data.{key}"))
}

/// Extract one value via dot notation with array indexing.
/// `"services[0].connection.uris[0]"` walks objects and arrays.
/// Returns the value as a string (objects/arrays serialize as JSON).

pub fn extract_json_field(value: &serde_json::Value, path: &str) -> Option<String> {
    let mut current = value;
    for segment in path.split('.') {
        if let Some(bracket_pos) = segment.find('[') {
            let field_name = &segment[..bracket_pos];
            let rest = &segment[bracket_pos..];
            if !field_name.is_empty() {
                current = current.get(field_name)?;
            }
            let mut chars = rest.chars().peekable();
            while chars.peek() == Some(&'[') {
                chars.next();
                let mut index_str = String::new();
                while let Some(&c) = chars.peek() {
                    if c == ']' {
                        chars.next();
                        break;
                    }
                    index_str.push(c);
                    chars.next();
                }
                let index: usize = index_str.parse().ok()?;
                current = current.get(index)?;
            }
        } else {
            current = current.get(segment)?;
        }
    }
    match current {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        serde_json::Value::Bool(b) => Some(b.to_string()),
        other => Some(other.to_string()),
    }
}

/// Parse repeated NAME=NUMBER flags into an ordered map.

pub fn parse_u16_pairs(raw: &[String]) -> Result<HashMap<String, u16>, String> {
    raw.iter()
        .map(|s| match s.split_once('=') {
            Some((k, v)) => v.parse::<u16>().map(|n| (k.to_string(), n)).map_err(|_| {
                format!("--port '{s}' must look like NAME=NUMBER (e.g. --port default=6379)")
            }),
            None => Err(format!(
                "--port '{s}' must look like NAME=PORT (e.g. --port default=6379)"
            )),
        })
        .collect()
}

/// Parse repeated KEY=VALUE flags; values may contain '=' themselves.

pub fn parse_input_map(raw: &[String]) -> Result<std::collections::BTreeMap<String, String>, String> {
    raw.iter()
        .map(|s| match s.split_once('=') {
            Some((k, v)) => Ok((k.trim().to_string(), v.to_string())),
            None => Err(format!(
                "--input '{s}' must look like KEY=VALUE (e.g. --input password=hunter2)"
            )),
        })
        .collect()
}

/// Delight of continuity: on success, tend toward this moss (unless the
/// attachment was explicitly pinned — those are already remembered).

pub fn parse_garden(v: &serde_json::Value) -> Result<Vec<GardenStone>, String> {
    let stones = envelope_plain(v)?.get("stones").cloned().ok_or_else(|| "garden view lacked data.stones".to_string())?;
    serde_json::from_value::<Vec<GardenStone>>(stones)
        .map_err(|e| format!("garden view did not match standard format: {e}"))
}


pub fn named_pairs(map: &serde_json::Value, sep: &str) -> String {
    map.as_object()
        .map(|pairs| {
            pairs
                .iter()
                .filter_map(|(k, v)| v.as_u64().map(|n| format!("{k} → {n}")))
                .collect::<Vec<_>>()
                .join(sep)
        })
        .unwrap_or_default()
}

// --- human renderings for stone ops -----------------------------------------

/// Surfaces suppress `::default` (infrastructure noise); foreign instances
/// stay in full (`ollama::adopted` is honest on the wire AND to humans).

pub fn describe_ports(container: &serde_json::Value, host: &serde_json::Value) -> String {
    let mut parts = Vec::new();
    if let Some(names) = container.as_object() {
        for (role, n) in names {
            match host.get(role).and_then(|h| h.as_u64()) {
                Some(host_port) => parts.push(format!("{role}: {n} → :{host_port}")),
                None => parts.push(format!("{role}: {n}")),
            }
        }
    }
    if parts.is_empty() {
        parts.push(named_pairs(host, ", "));
    }
    parts.join(", ")
}

/// §5.3's placed record rendered by hand — the delightful reference.

pub fn render_status(verb_past: &str, data: &serde_json::Value) -> Result<(), String> {
    let name = display_name(data["name"].as_str().unwrap_or("(unnamed)"));
    let status = data["status"].as_str().unwrap_or("?");
    println!("{name} {verb_past} — {status}");
    Ok(())
}


