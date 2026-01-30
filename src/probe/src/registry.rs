//! Test Registry - discovers and registers tests for on-demand execution

use crate::{Bag, LiveGarden, TestReport};
use anyhow::Result;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// Test function signature - takes garden and bag, returns updated bag
pub type TestFn = fn(
    Arc<LiveGarden>,
    Bag,
) -> Pin<Box<dyn Future<Output = Result<Bag>> + Send>>;

/// Test definition
#[derive(Clone)]
pub struct TestDef {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub category: &'static str,
    pub tags: &'static [&'static str],
    pub run: TestFn,
}

/// Registry of all available tests
pub struct TestRegistry {
    tests: HashMap<String, TestDef>,
}

impl TestRegistry {
    /// Build registry with all known tests
    pub fn new() -> Self {
        let mut registry = Self {
            tests: HashMap::new(),
        };

        // Register all tests manually
        registry.register_all();

        registry
    }

    fn register(&mut self, def: TestDef) {
        self.tests.insert(def.id.to_string(), def);
    }

    fn register_all(&mut self) {
        // Smoke tests
        self.register(crate::tests::smoke::health_test());
        self.register(crate::tests::smoke::capabilities_test());
        self.register(crate::tests::smoke::offerings_list_test());

        // Discovery tests
        self.register(crate::tests::discovery::topology_test());
        self.register(crate::tests::discovery::stone_count_test());
        self.register(crate::tests::discovery::tended_test());

        // Tend tests (tending selection, fallback, persistence)
        self.register(crate::tests::tend::reachable_test());
        self.register(crate::tests::tend::capabilities_test());
        self.register(crate::tests::tend::alternatives_test());
        self.register(crate::tests::tend::switch_simulation_test());
        self.register(crate::tests::tend::all_healthy_test());
        self.register(crate::tests::tend::round_robin_test());

        // Inter-stone tests (communication between stones)
        self.register(crate::tests::interstone::discovery_consistency_test());
        self.register(crate::tests::interstone::cross_query_test());
        self.register(crate::tests::interstone::offering_visibility_test());
        self.register(crate::tests::interstone::latency_matrix_test());

        // Offerings tests (catalog, search, deployment)
        self.register(crate::tests::offerings::catalog_test());
        self.register(crate::tests::offerings::installed_test());
        self.register(crate::tests::offerings::search_test());
        self.register(crate::tests::offerings::services_match_topology_test());

        // Nourishment tests (updates detection)
        self.register(crate::tests::nourishment::detection_test());
        self.register(crate::tests::nourishment::garden_aggregation_test());
        self.register(crate::tests::nourishment::scope_filtering_test());

        // Adapter tests (registry, commands)
        self.register(crate::tests::adapters::registry_test());
        self.register(crate::tests::adapters::cricket_test());
        self.register(crate::tests::adapters::command_forwarding_test());

        // Storage tests (seed banks, beacon)
        self.register(crate::tests::storage::overview_test());
        self.register(crate::tests::storage::candidates_test());
        self.register(crate::tests::storage::beacon_visibility_test());
        self.register(crate::tests::storage::object_roundtrip_test());

        // Nurturing tests (A/B backup slots, sub-capabilities)
        self.register(crate::tests::nurturing::index_test());
        self.register(crate::tests::nurturing::offering_slots_test());
        self.register(crate::tests::nurturing::create_snapshot_test());
        self.register(crate::tests::nurturing::subcap_discovery_test());
        self.register(crate::tests::nurturing::find_with_subcap_test());
        self.register(crate::tests::nurturing::offering_id_stability_test());
        self.register(crate::tests::nurturing::remote_list_test());

        // Resolution tests (protocol and offering resolution)
        self.register(crate::tests::resolution::offering_lookup_test());
        self.register(crate::tests::resolution::protocol_test());
        self.register(crate::tests::resolution::consistency_test());
    }

    /// Get all test definitions
    pub fn all(&self) -> Vec<&TestDef> {
        self.tests.values().collect()
    }

    /// Get tests by category
    pub fn by_category(&self, category: &str) -> Vec<&TestDef> {
        self.tests
            .values()
            .filter(|t| t.category == category)
            .collect()
    }

    /// Get tests by tag
    pub fn by_tag(&self, tag: &str) -> Vec<&TestDef> {
        self.tests
            .values()
            .filter(|t| t.tags.contains(&tag))
            .collect()
    }

    /// Get all unique categories
    pub fn categories(&self) -> Vec<&str> {
        let mut cats: Vec<&str> = self.tests.values().map(|t| t.category).collect();
        cats.sort();
        cats.dedup();
        cats
    }

    /// Get a test by ID
    pub fn get(&self, id: &str) -> Option<&TestDef> {
        self.tests.get(id)
    }

    /// Find tests matching a pattern (supports glob-like *)
    pub fn find(&self, pattern: &str) -> Vec<&TestDef> {
        if pattern.contains('*') {
            let prefix = pattern.trim_end_matches('*');
            self.tests
                .values()
                .filter(|t| t.id.starts_with(prefix))
                .collect()
        } else {
            self.tests.get(pattern).map(|t| vec![t]).unwrap_or_default()
        }
    }

    /// Run a single test
    pub async fn run_test(
        &self,
        id: &str,
        garden: Arc<LiveGarden>,
        initial_bag: Bag,
    ) -> Result<TestReport> {
        let test = self
            .get(id)
            .ok_or_else(|| anyhow::anyhow!("Test not found: {}", id))?;

        let start = std::time::Instant::now();
        let result = (test.run)(garden, initial_bag).await;
        let duration = start.elapsed();

        match result {
            Ok(bag) => Ok(TestReport {
                test_id: id.to_string(),
                test_name: test.name.to_string(),
                success: bag.all_passed(),
                duration_ms: duration.as_millis() as u64,
                bag,
                error: None,
            }),
            Err(e) => {
                let mut bag = Bag::new();
                bag.record_step(
                    "error",
                    "Test execution failed",
                    duration.as_millis() as u64,
                    crate::StepResult::failed(e.to_string()),
                );

                Ok(TestReport {
                    test_id: id.to_string(),
                    test_name: test.name.to_string(),
                    success: false,
                    duration_ms: duration.as_millis() as u64,
                    bag,
                    error: Some(e.to_string()),
                })
            }
        }
    }

    /// Run multiple tests
    pub async fn run_tests(&self, ids: &[&str], garden: Arc<LiveGarden>) -> Vec<TestReport> {
        let mut reports = Vec::new();

        for id in ids {
            let bag = Bag::new();
            let report = self.run_test(id, garden.clone(), bag).await;
            match report {
                Ok(r) => reports.push(r),
                Err(e) => {
                    eprintln!("Failed to run test {}: {}", id, e);
                }
            }
        }

        reports
    }
}

impl Default for TestRegistry {
    fn default() -> Self {
        Self::new()
    }
}
