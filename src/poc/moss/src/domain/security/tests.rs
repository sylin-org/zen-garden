//! Security aggregate unit tests.

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::domain::ceremony::CeremonyRegistry;
    use crate::domain::metrics::Metrics;
    use crate::domain::security::aggregate::Security;
    use crate::domain::security::ceremony_persistence::CeremonyPersistence;
    use crate::domain::security::pond_client::PondClient;

    // ── Fakes ────────────────────────────────────────────────────────

    struct NoopPondClient;

    impl PondClient for NoopPondClient {
        fn get(
            &self,
            _address: &garden_common::PeerAddress,
            _path: &str,
        ) -> reqwest::RequestBuilder {
            reqwest::Client::new().get("http://localhost")
        }
        fn post(
            &self,
            _address: &garden_common::PeerAddress,
            _path: &str,
        ) -> reqwest::RequestBuilder {
            reqwest::Client::new().post("http://localhost")
        }
        fn put(
            &self,
            _address: &garden_common::PeerAddress,
            _path: &str,
        ) -> reqwest::RequestBuilder {
            reqwest::Client::new().put("http://localhost")
        }
        fn delete(
            &self,
            _address: &garden_common::PeerAddress,
            _path: &str,
        ) -> reqwest::RequestBuilder {
            reqwest::Client::new().delete("http://localhost")
        }
        fn reload_tls(&self) {}
    }

    struct NoopCeremonyJournal;

    impl CeremonyPersistence for NoopCeremonyJournal {
        fn persist<'a>(
            &'a self,
            _ceremony: &'a crate::domain::ceremony::Ceremony,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send + 'a>>
        {
            Box::pin(async { Ok(()) })
        }
        fn load_active(
            &self,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<
                        Output = anyhow::Result<Vec<crate::domain::ceremony::Ceremony>>,
                    > + Send
                    + '_,
            >,
        > {
            Box::pin(async { Ok(Vec::new()) })
        }
        fn load<'a>(
            &'a self,
            _id: &'a crate::domain::ceremony::CeremonyId,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<
                        Output = anyhow::Result<Option<crate::domain::ceremony::Ceremony>>,
                    > + Send
                    + 'a,
            >,
        > {
            Box::pin(async { Ok(None) })
        }
        fn remove<'a>(
            &'a self,
            _id: &'a crate::domain::ceremony::CeremonyId,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send + 'a>>
        {
            Box::pin(async { Ok(()) })
        }
        fn prune_archive(
            &self,
            _older_than: chrono::Duration,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<usize>> + Send + '_>>
        {
            Box::pin(async { Ok(0) })
        }
    }

    async fn test_aggregate() -> Security<NoopPondClient> {
        let metrics = Arc::new(Metrics::new());
        Security::new(
            Arc::new(std::sync::atomic::AtomicBool::new(false)),
            Arc::new(NoopPondClient),
            Arc::new(koi_common::ceremony::CeremonyHost::new(
                koi_certmesh::init_ceremony::InitCeremonyRules::new(
                    koi_certmesh::CertmeshPaths::with_data_dir(
                        std::env::temp_dir().join("zen-garden-security-test").join("koi"),
                    ),
                ),
            )),
            Arc::new(CeremonyRegistry::new()),
            Arc::new(NoopCeremonyJournal),
            metrics,
        )
        .await
    }

    // ── Query tests ──────────────────────────────────────────────────

    #[tokio::test]
    async fn new_aggregate_is_unenrolled() {
        let security = test_aggregate().await;
        assert!(!security.enrolled());
        assert!(!security.pond_active());
        assert!(security.cornerstone().await.is_none());
        assert!(security.pond_name().await.is_none());
        assert!(!security.https_started());
    }

    // ── Command tests ────────────────────────────────────────────────

    #[tokio::test]
    async fn mark_enrolled_emits_event_and_updates_state() {
        let security = test_aggregate().await;
        let mut rx = security.changes();

        let changed = security
            .mark_enrolled(Some("stone-alpha".to_string()))
            .await;

        assert!(changed);
        assert!(security.enrolled());
        assert!(security.pond_active());
        assert_eq!(
            security.cornerstone().await,
            Some("stone-alpha".to_string())
        );

        let event = rx.try_recv().unwrap();
        assert_eq!(event.kind.name(), "enrolled");
    }

    #[tokio::test]
    async fn mark_enrolled_twice_returns_false() {
        let security = test_aggregate().await;

        let first = security.mark_enrolled(Some("stone-a".to_string())).await;
        let second = security.mark_enrolled(Some("stone-b".to_string())).await;

        assert!(first);
        assert!(!second);
        // Cornerstone is updated even if already enrolled
        assert_eq!(security.cornerstone().await, Some("stone-b".to_string()));
    }

    #[tokio::test]
    async fn mark_unenrolled_emits_event_and_clears_state() {
        let security = test_aggregate().await;
        security.mark_enrolled(Some("stone-a".to_string())).await;
        security.set_pond_name("pond-still-lotus".to_string()).await;

        let mut rx = security.changes();
        let changed = security.mark_unenrolled().await;

        assert!(changed);
        assert!(!security.enrolled());
        assert!(!security.pond_active());
        assert!(security.cornerstone().await.is_none());
        assert!(security.pond_name().await.is_none());

        let event = rx.try_recv().unwrap();
        assert_eq!(event.kind.name(), "unenrolled");
    }

    #[tokio::test]
    async fn mark_unenrolled_when_not_enrolled_returns_false() {
        let security = test_aggregate().await;
        let changed = security.mark_unenrolled().await;
        assert!(!changed);
    }

    #[tokio::test]
    async fn set_pond_name_emits_event() {
        let security = test_aggregate().await;
        let mut rx = security.changes();

        security.set_pond_name("pond-quiet-creek".to_string()).await;

        assert_eq!(
            security.pond_name().await,
            Some("pond-quiet-creek".to_string())
        );

        let event = rx.try_recv().unwrap();
        assert_eq!(event.kind.name(), "pond_renamed");
    }

    #[tokio::test]
    async fn refresh_active_does_not_emit_event() {
        let security = test_aggregate().await;
        let mut rx = security.changes();

        security.refresh_active(true);
        assert!(security.pond_active());

        // No event should have been emitted
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn https_started_lifecycle() {
        let security = test_aggregate().await;
        assert!(!security.https_started());

        security.set_https_started();
        assert!(security.https_started());

        security.clear_https_started();
        assert!(!security.https_started());
    }

    #[tokio::test]
    async fn seed_state_does_not_emit_event() {
        let security = test_aggregate().await;
        let mut rx = security.changes();

        security
            .seed_state(
                true,
                Some("stone-cornerstone".to_string()),
                Some("pond-zen-garden".to_string()),
            )
            .await;

        assert!(security.enrolled());
        assert_eq!(
            security.cornerstone().await,
            Some("stone-cornerstone".to_string())
        );
        assert_eq!(
            security.pond_name().await,
            Some("pond-zen-garden".to_string())
        );

        // No event should have been emitted during seeding
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn recover_ceremonies_returns_zero_with_noop_journal() {
        let security = test_aggregate().await;
        let count = security.recover_ceremonies().await.unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn stone_client_accessible() {
        let security = test_aggregate().await;
        // Just verify it doesn't panic and returns the injected client
        let _client = security.stone_client();
    }

    #[tokio::test]
    async fn ceremony_registry_accessible() {
        let security = test_aggregate().await;
        let registry = security.ceremony_registry();
        assert_eq!(registry.active_count().await, 0);
    }
}
