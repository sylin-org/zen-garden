//! MongoDB wire protocol client using the official `mongodb` driver.
//!
//! Wraps a `mongodb::Client` connection pool and exposes replica-set
//! administration commands as typed methods.

use anyhow::{Context, Result};
use mongodb::bson::{doc, Document};
use mongodb::options::ClientOptions;

/// Wire protocol client for a single MongoDB endpoint.
pub struct MongoClient {
    client: mongodb::Client,
    endpoint: String,
}

/// Parsed replica set status from `replSetGetStatus`.
#[derive(Debug, Clone)]
pub struct RsStatus {
    /// Replica set name.
    pub set_name: String,
    /// Member states.
    pub members: Vec<RsMember>,
}

/// A single member from `replSetGetStatus.members[]`.
#[derive(Debug, Clone)]
pub struct RsMember {
    /// Member endpoint (e.g. "192.168.1.5:27017").
    pub name: String,
    /// Numeric state code (1=PRIMARY, 2=SECONDARY, 7=ARBITER, etc.).
    pub state: i32,
    /// Human-readable state string.
    pub state_str: String,
    /// Whether the member is healthy.
    pub health: f64,
    /// Optime timestamp (seconds since epoch).
    pub optime_ts: Option<i64>,
    /// Last heartbeat received.
    pub last_heartbeat: Option<chrono::DateTime<chrono::Utc>>,
    /// `true` if this member is the one we ran the command against.
    pub is_self: bool,
}

/// Oplog window information from `getReplicationInfo`-equivalent queries.
#[derive(Debug, Clone)]
pub struct ReplicationInfo {
    /// Oplog size in MB.
    pub oplog_size_mb: f64,
    /// Oplog used in MB.
    pub oplog_used_mb: f64,
    /// Oplog window in seconds.
    pub oplog_window_secs: f64,
}

impl MongoClient {
    /// Connect to a MongoDB endpoint.
    ///
    /// `endpoint` should be like `"192.168.1.5:27017"` (no `mongodb://` prefix).
    pub async fn connect(endpoint: &str) -> Result<Self> {
        let uri = format!("mongodb://{endpoint}/?directConnection=true&serverSelectionTimeoutMS=5000&connectTimeoutMS=5000");
        let options = ClientOptions::parse(&uri)
            .await
            .with_context(|| format!("parse MongoDB connection string for {endpoint}"))?;

        let client = mongodb::Client::with_options(options)
            .with_context(|| format!("create MongoDB client for {endpoint}"))?;

        Ok(Self {
            client,
            endpoint: endpoint.to_string(),
        })
    }

    /// Run `replSetGetStatus` and parse the result.
    pub async fn rs_status(&self) -> Result<RsStatus> {
        let db = self.client.database("admin");
        let result = db
            .run_command(doc! { "replSetGetStatus": 1 })
            .await
            .with_context(|| format!("replSetGetStatus on {}", self.endpoint))?;

        let set_name = result
            .get_str("set")
            .unwrap_or("unknown")
            .to_string();

        let members = match result.get_array("members") {
            Ok(arr) => arr
                .iter()
                .filter_map(|m| {
                    let doc = m.as_document()?;
                    Some(RsMember {
                        name: doc.get_str("name").unwrap_or("").to_string(),
                        state: doc.get_i32("state").unwrap_or(0),
                        state_str: doc.get_str("stateStr").unwrap_or("UNKNOWN").to_string(),
                        health: doc.get_f64("health").unwrap_or(0.0),
                        optime_ts: doc
                            .get_document("optime")
                            .ok()
                            .and_then(|o| o.get("ts"))
                            .and_then(|ts| ts.as_timestamp())
                            .map(|t| t.time as i64),
                        last_heartbeat: doc
                            .get_datetime("lastHeartbeat")
                            .ok()
                            .map(|dt| {
                                let millis = dt.timestamp_millis();
                                chrono::DateTime::from_timestamp_millis(millis)
                                    .unwrap_or_default()
                            }),
                        is_self: doc.get_bool("self").unwrap_or(false),
                    })
                })
                .collect(),
            Err(_) => vec![],
        };

        Ok(RsStatus { set_name, members })
    }

    /// Initialize a new replica set with this node as the sole member.
    ///
    /// Equivalent to `rs.initiate({_id: rs_name, members: [{_id: 0, host: self.endpoint}]})`.
    ///
    /// Returns `Ok(true)` if freshly initiated, `Ok(false)` if already initialized
    /// (MongoDB error code 23 / `AlreadyInitialized`).
    pub async fn rs_initiate(&self, rs_name: &str) -> Result<bool> {
        let config = doc! {
            "_id": rs_name,
            "members": [
                { "_id": 0, "host": &self.endpoint }
            ]
        };

        let db = self.client.database("admin");
        let result = match db
            .run_command(doc! { "replSetInitiate": config })
            .await
        {
            Ok(doc) => doc,
            Err(e) => {
                let err_str = e.to_string();
                // AlreadyInitialized (code 23) — the replica set exists, treat as success
                if err_str.contains("AlreadyInitialized") || err_str.contains("already initialized")
                {
                    tracing::info!(
                        endpoint = %self.endpoint,
                        rs_name = %rs_name,
                        "replica set already initialized — treating as success"
                    );
                    return Ok(false);
                }
                return Err(e).with_context(|| format!("replSetInitiate on {}", self.endpoint));
            }
        };

        // Check for success (ok: 1.0)
        let ok = result.get_f64("ok").unwrap_or(0.0);
        if ok != 1.0 {
            let errmsg = result.get_str("errmsg").unwrap_or("unknown error");
            // AlreadyInitialized can also come through the ok!=1 path
            if errmsg.contains("already initialized") {
                tracing::info!(
                    endpoint = %self.endpoint,
                    rs_name = %rs_name,
                    "replica set already initialized — treating as success"
                );
                return Ok(false);
            }
            anyhow::bail!(
                "replSetInitiate failed on {}: {}",
                self.endpoint,
                errmsg
            );
        }

        Ok(true)
    }

    /// Add a new member to the replica set.
    ///
    /// Must be run against the PRIMARY. Equivalent to `rs.add(host)`.
    pub async fn rs_add(&self, host: &str) -> Result<()> {
        // Get current config
        let db = self.client.database("admin");
        let config_doc = db
            .run_command(doc! { "replSetGetConfig": 1 })
            .await
            .context("replSetGetConfig")?;

        let mut config = config_doc
            .get_document("config")
            .context("missing config in replSetGetConfig response")?
            .clone();

        // Increment version
        let version = config.get_i32("version").unwrap_or(0);
        config.insert("version", version + 1);

        // Add new member with next _id
        let members = config
            .get_array_mut("members")
            .context("missing members in replica set config")?;

        let max_id = members
            .iter()
            .filter_map(|m| m.as_document())
            .filter_map(|d| d.get_i32("_id").ok())
            .max()
            .unwrap_or(-1);

        members.push(mongodb::bson::Bson::Document(doc! {
            "_id": max_id + 1,
            "host": host,
        }));

        // Reconfigure
        let result = db
            .run_command(doc! { "replSetReconfig": config })
            .await
            .with_context(|| format!("replSetReconfig (adding {host})"))?;

        let ok = result.get_f64("ok").unwrap_or(0.0);
        if ok != 1.0 {
            let errmsg = result.get_str("errmsg").unwrap_or("unknown error");
            anyhow::bail!("replSetReconfig failed: {errmsg}");
        }

        Ok(())
    }

    /// Remove a member from the replica set.
    ///
    /// Must be run against the PRIMARY.
    pub async fn rs_remove(&self, host: &str) -> Result<()> {
        let db = self.client.database("admin");
        let config_doc = db
            .run_command(doc! { "replSetGetConfig": 1 })
            .await
            .context("replSetGetConfig")?;

        let mut config = config_doc
            .get_document("config")
            .context("missing config in replSetGetConfig response")?
            .clone();

        let version = config.get_i32("version").unwrap_or(0);
        config.insert("version", version + 1);

        let members = config
            .get_array_mut("members")
            .context("missing members in replica set config")?;

        members.retain(|m| {
            m.as_document()
                .and_then(|d| d.get_str("host").ok())
                .map(|h| h != host)
                .unwrap_or(true)
        });

        let result = db
            .run_command(doc! { "replSetReconfig": config })
            .await
            .with_context(|| format!("replSetReconfig (removing {host})"))?;

        let ok = result.get_f64("ok").unwrap_or(0.0);
        if ok != 1.0 {
            let errmsg = result.get_str("errmsg").unwrap_or("unknown error");
            anyhow::bail!("replSetReconfig failed: {errmsg}");
        }

        Ok(())
    }

    /// Rewrite the replica set member list to match the given endpoints.
    ///
    /// Fetches the current RS config, preserves `_id` for any host that already
    /// exists (MongoDB requires stable member IDs across reconfigs), assigns new
    /// IDs only for new hosts, increments the version, and applies via
    /// `replSetReconfig` with `force: true`.
    pub async fn rs_reconfig_members(&self, rs_name: &str, desired_hosts: &[&str]) -> Result<()> {
        self.rs_reconfig_members_with_mapping(rs_name, desired_hosts, &std::collections::HashMap::new()).await
    }

    /// Rewrite the replica set member list, preserving `_id` across IP changes.
    ///
    /// `old_to_new` maps old endpoints (currently in RS config) to their
    /// replacement endpoints. This allows preserving MongoDB member `_id`s
    /// when a stone's IP changes due to DHCP renewal — the old endpoint's
    /// `_id` is reused for the new endpoint.
    pub async fn rs_reconfig_members_with_mapping(
        &self,
        rs_name: &str,
        desired_hosts: &[&str],
        old_to_new: &std::collections::HashMap<String, String>,
    ) -> Result<()> {
        let db = self.client.database("admin");
        let config_doc = db
            .run_command(doc! { "replSetGetConfig": 1 })
            .await
            .context("replSetGetConfig")?;

        let old_config = config_doc
            .get_document("config")
            .context("missing config in replSetGetConfig response")?;

        let version = old_config.get_i32("version").unwrap_or(0);

        // Build host→_id map from existing config so we preserve IDs.
        // Also register old→new mappings: if old endpoint "192.168.1.5:27017"
        // maps to new "192.168.1.6:27017", the _id for .5 is reused for .6.
        let mut existing_ids: std::collections::HashMap<String, i32> =
            std::collections::HashMap::new();
        let mut max_id: i32 = -1;
        if let Ok(old_members) = old_config.get_array("members") {
            for m in old_members {
                if let Some(doc) = m.as_document() {
                    if let (Ok(host), Ok(id)) = (doc.get_str("host"), doc.get_i32("_id")) {
                        existing_ids.insert(host.to_string(), id);
                        // If this old host has a replacement, map the new host to the same _id
                        if let Some(new_host) = old_to_new.get(host) {
                            existing_ids.insert(new_host.clone(), id);
                        }
                        max_id = max_id.max(id);
                    }
                }
            }
        }

        // Build new member list — reuse existing _id where possible
        let mut next_id = max_id + 1;
        let new_members: Vec<mongodb::bson::Bson> = desired_hosts
            .iter()
            .map(|host| {
                let id = existing_ids.get(*host).copied().unwrap_or_else(|| {
                    let id = next_id;
                    next_id += 1;
                    id
                });
                mongodb::bson::Bson::Document(doc! {
                    "_id": id,
                    "host": *host,
                })
            })
            .collect();

        let new_config = doc! {
            "_id": rs_name,
            "version": version + 1,
            "members": new_members,
        };

        tracing::debug!(config = %new_config, "sending replSetReconfig");
        let result = db
            .run_command(doc! { "replSetReconfig": new_config, "force": true })
            .await
            .with_context(|| format!("replSetReconfig (rebuild members) on {}", self.endpoint))?;

        let ok = result.get_f64("ok").unwrap_or(0.0);
        if ok != 1.0 {
            let errmsg = result.get_str("errmsg").unwrap_or("unknown error");
            anyhow::bail!("replSetReconfig failed: {errmsg}");
        }

        Ok(())
    }

    /// Ask the primary to step down for the given number of seconds.
    pub async fn rs_step_down(&self, seconds: u32) -> Result<()> {
        let db = self.client.database("admin");
        // stepDown may return an error as the connection gets closed — that's expected
        let _ = db
            .run_command(doc! { "replSetStepDown": seconds as i64 })
            .await;
        Ok(())
    }

    /// Run `serverStatus` and return the raw document.
    pub async fn server_status(&self) -> Result<Document> {
        let db = self.client.database("admin");
        db.run_command(doc! { "serverStatus": 1 })
            .await
            .with_context(|| format!("serverStatus on {}", self.endpoint))
    }

    /// Get oplog replication info.
    ///
    /// Queries the `local.oplog.rs` collection stats and first/last entries
    /// to compute the oplog window.
    pub async fn replication_info(&self) -> Result<ReplicationInfo> {
        let db = self.client.database("local");

        // Get oplog collection stats
        let stats = self
            .client
            .database("admin")
            .run_command(doc! {
                "collStats": "oplog.rs",
                "scale": 1_048_576_i32, // MB
            })
            .await
            .context("collStats oplog.rs")?;

        let oplog_size_mb = stats.get_f64("maxSize").unwrap_or(0.0);
        let oplog_used_mb = stats.get_f64("size").unwrap_or(0.0);

        // Get first and last oplog entries to compute window
        let oplog = db.collection::<Document>("oplog.rs");

        use mongodb::options::FindOneOptions;

        let first = oplog
            .find_one(doc! {})
            .with_options(FindOneOptions::builder().sort(doc! { "$natural": 1 }).build())
            .await
            .context("find first oplog entry")?;

        let last = oplog
            .find_one(doc! {})
            .with_options(FindOneOptions::builder().sort(doc! { "$natural": -1 }).build())
            .await
            .context("find last oplog entry")?;

        let oplog_window_secs = match (first, last) {
            (Some(f), Some(l)) => {
                let first_ts = f
                    .get("ts")
                    .and_then(|v| v.as_timestamp())
                    .map(|t| t.time as f64)
                    .unwrap_or(0.0);
                let last_ts = l
                    .get("ts")
                    .and_then(|v| v.as_timestamp())
                    .map(|t| t.time as f64)
                    .unwrap_or(0.0);
                last_ts - first_ts
            }
            _ => 0.0,
        };

        Ok(ReplicationInfo {
            oplog_size_mb,
            oplog_used_mb,
            oplog_window_secs,
        })
    }

    /// Read the replica set config via `replSetGetConfig`.
    ///
    /// Unlike `rs_status()`, this succeeds even when the node reports
    /// `InvalidReplicaSetConfig` (error 93) — the local config is still
    /// readable even though the node isn't an active member.
    ///
    /// Returns `(rs_name, members)` where each member has its MongoDB
    /// `_id` and host string.
    pub async fn rs_config(&self) -> Result<(String, Vec<RsConfigMember>)> {
        let db = self.client.database("admin");
        let result = db
            .run_command(doc! { "replSetGetConfig": 1 })
            .await
            .with_context(|| format!("replSetGetConfig on {}", self.endpoint))?;

        let config = result
            .get_document("config")
            .context("missing config in replSetGetConfig response")?;

        let rs_name = config
            .get_str("_id")
            .unwrap_or("unknown")
            .to_string();

        let members = match config.get_array("members") {
            Ok(arr) => arr
                .iter()
                .filter_map(|m| {
                    let doc = m.as_document()?;
                    Some(RsConfigMember {
                        id: doc.get_i32("_id").ok()?,
                        host: doc.get_str("host").ok()?.to_string(),
                    })
                })
                .collect(),
            Err(_) => vec![],
        };

        Ok((rs_name, members))
    }

    /// Ping the server to check connectivity.
    pub async fn ping(&self) -> bool {
        let db = self.client.database("admin");
        db.run_command(doc! { "ping": 1 }).await.is_ok()
    }
}

/// A member entry from `replSetGetConfig`.
#[derive(Debug, Clone)]
pub struct RsConfigMember {
    /// MongoDB member `_id` (stable across reconfigs).
    pub id: i32,
    /// Host endpoint (e.g. `"192.168.1.5:27017"`).
    pub host: String,
}
