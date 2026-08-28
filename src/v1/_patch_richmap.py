import io

# --- contract/discovery.rs: inventory map replaces the services field ---
p = 'crates/contract/src/discovery.rs'
src = io.open(p, encoding='utf-8').read()
old = """/// Where a willing respondent lives, and (when the ask was rich) what it
/// hosts. The `stone:` block always answers "who are you"; the inventory
/// answers "what do you have" - identical shapes to the chirp frame's.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiscoveryResponse {
    /// WHO answered: identity and reachability (frame's `stone:` block).
    pub stone: crate::chirp::Stone,
    /// Legacy Lantern registry endpoint (v0 field; v1 emits absent).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lantern_endpoint: Option<String>,
    /// Inventory present iff the request carried rich:true.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub services: Option<crate::chirp::Inventory<crate::chirp::ServiceEntry>>,
}"""
new = """/// Where a willing respondent lives, and (when the ask was rich) what it
/// hosts. The `stone:` block always answers "who are you"; the inventory
/// MAP answers "what do you have" - every domain, identical shapes to the
/// chirp frame's (A2.1: the revision vector is a shape, not a field list).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiscoveryResponse {
    /// WHO answered: identity and reachability (frame's `stone:` block).
    pub stone: crate::chirp::Stone,
    /// Legacy Lantern registry endpoint (v0 field; v1 emits absent).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lantern_endpoint: Option<String>,
    /// The full inventory map, iff the request carried rich:true - services
    /// AND banks AND whatever domains the future brings (W7 finding: a
    /// newcomer learns the whole room in one exchange).
    #[serde(default, skip_serializing_if = "crate::chirp::InventoryMap::is_empty")]
    pub inventory: crate::chirp::InventoryMap,
}"""
assert src.count(old) == 1, "discovery response"
src = src.replace(old, new)
io.open(p, 'w', encoding='utf-8', newline='').write(src)
print("discovery.rs done")

# --- contract/chirp.rs: is_empty helper ---
p = 'crates/contract/src/chirp.rs'
src = io.open(p, encoding='utf-8').read()
old = """    /// Merge `newer` over `self` per-domain by revision (A2.1): absent key
    /// keeps what we have; present block's rev decides."""
new = """    /// True when the map says nothing about any domain (skip on the wire).
    pub fn is_empty(&self) -> bool {
        self.services.is_none() && self.banks.is_none() && self.extra.is_empty()
    }

    /// Merge `newer` over `self` per-domain by revision (A2.1): absent key
    /// keeps what we have; present block's rev decides."""
assert src.count(old) == 1, "is_empty"
src = src.replace(old, new)
io.open(p, 'w', encoding='utf-8', newline='').write(src)
print("chirp.rs done")

# --- responder.rs: compose the full map ---
p = 'crates/kernel/src/responder.rs'
src = io.open(p, encoding='utf-8').read()
old = """    let body = source.body();
    let services = if rich {
        source
            .song_blocks()
            .into_iter()
            .find(|(domain, _)| domain == garden_contract::chirp::DOMAIN_SERVICES)
            .and_then(|(_, block)| {
                serde_json::from_value::<garden_contract::chirp::Inventory<
                    garden_contract::chirp::ServiceEntry,
                >>(block)
                .ok()
            })
    } else {
        None
    };
    let response = DiscoveryResponse {
        stone: body.stone.clone(),
        lantern_endpoint: None,
        services,
    };"""
new = """    let body = source.body();
    let inventory = if rich {
        // Every domain the stone has something to say about (A2.1).
        garden_contract::chirp::InventoryMap::from_pairs(source.song_blocks())
    } else {
        garden_contract::chirp::InventoryMap::default()
    };
    let response = DiscoveryResponse {
        stone: body.stone.clone(),
        lantern_endpoint: None,
        inventory,
    };"""
assert src.count(old) == 1, "responder"
src = src.replace(old, new)

# responder tests: FixedSource unchanged; assertions move to inventory map
old = """    #[tokio::test]
    async fn rich_ask_gets_the_inventory() {
        let resp = answered_for(DiscoveryRequest::for_moss_rich("tester")).await;
        let inv = resp.services.expect("rich ask earns the inventory");
        assert_eq!(inv.rev, Some(4));
        assert_eq!(inv.items[0].name, "memcached::default");
        assert_eq!(resp.stone.name, "stone-tells");
    }

    #[tokio::test]
    async fn lean_ask_gets_the_card_only() {
        let resp = answered_for(DiscoveryRequest::for_moss("tester")).await;
        assert!(resp.services.is_none(), "lean asks must not pay fat replies");
        assert_eq!(resp.stone.id, "sid-answer");
    }"""
new = """    #[tokio::test]
    async fn rich_ask_gets_the_inventory() {
        let resp = answered_for(DiscoveryRequest::for_moss_rich("tester")).await;
        let services = resp.inventory.services.expect("rich ask earns the map");
        assert_eq!(services.rev, Some(4));
        assert_eq!(services.items[0].name, "memcached::default");
        assert!(resp.inventory.banks.is_none(), "the double speaks one domain only");
        assert_eq!(resp.stone.name, "stone-tells");
    }

    #[tokio::test]
    async fn lean_ask_gets_the_card_only() {
        let resp = answered_for(DiscoveryRequest::for_moss("tester")).await;
        assert!(resp.inventory.is_empty(), "lean asks must not pay fat replies");
        assert_eq!(resp.stone.id, "sid-answer");
    }"""
assert src.count(old) == 1, "responder tests"
src = src.replace(old, new)

old = """        let resp = answered_with_raw(serde_json::json!({"discover": 42})).await;
        assert!(resp.services.is_none(), "no depth can be earned by garbage");"""
new = """        let resp = answered_with_raw(serde_json::json!({"discover": 42})).await;
        assert!(resp.inventory.is_empty(), "no depth can be earned by garbage");"""
assert src.count(old) == 1, "garbage test"
src = src.replace(old, new)
io.open(p, 'w', encoding='utf-8', newline='').write(src)
print("responder.rs done")
