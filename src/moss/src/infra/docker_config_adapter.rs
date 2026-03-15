//! Adapter: implements `DockerConfigOps` trait using the existing
//! `docker_config` module's free functions.

use crate::domain::traits::DockerConfigOps;
use crate::infra::docker_config;
use anyhow::Result;
use async_trait::async_trait;

/// Concrete `DockerConfigOps` backed by filesystem operations on daemon.json.
pub struct OsDockerConfig;

#[async_trait]
impl DockerConfigOps for OsDockerConfig {
    async fn read_insecure_registries(&self) -> Result<Vec<String>> {
        docker_config::read_insecure_registries().await
    }

    async fn write_insecure_registries(&self, registries: &[String]) -> Result<bool> {
        docker_config::write_insecure_registries(registries).await
    }

    async fn read_garden_registries(&self) -> Vec<String> {
        docker_config::read_garden_registries().await
    }

    async fn write_garden_registries(&self, registries: &[String]) -> Result<()> {
        docker_config::write_garden_registries(registries).await
    }

    async fn restart_docker_daemon(&self) -> Result<()> {
        docker_config::restart_docker_daemon().await
    }
}
