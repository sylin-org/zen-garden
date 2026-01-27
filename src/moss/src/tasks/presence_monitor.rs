//! Stone metrics monitoring for presence protocol
//!
//! Emits StoneEvent::LoadUpdated every 5 seconds to EventBus.

use std::time::Duration;
use tokio::time::interval;
use chrono::Utc;

use crate::{AppState, MossEvent};

/// Run load monitoring task (every 5s)
/// 
/// TODO: When EventBus is integrated, emit DomainEvent::Stone(StoneEvent::LoadUpdated)
/// For now, emit MossEvent for backward compatibility.
pub async fn run_load_monitor_task(state: AppState) {
    let mut interval = interval(Duration::from_secs(5));
    
    loop {
        interval.tick().await;
        
        // Get real system metrics from shared cache
        let (cpu_percent, memory_percent, disk_percent) = {
            let resources = state.system_resources.read().await;
            if let Some(ref res) = *resources {
                // Use primary mount point for load summary
                let primary_disk = res.storage.iter()
                    .find(|s| s.mount_point == "/" || s.mount_point == "C:\\\\")
                    .or_else(|| res.storage.iter().max_by_key(|s| s.total_gb))
                    .map(|s| s.used_percent)
                    .unwrap_or(0.0);
                
                (
                    res.cpu.usage_percent,
                    res.memory.used_percent,
                    primary_disk,
                )
            } else {
                (0.0, 0.0, 0.0)
            }
        };
        
        // Emit to event stream
        let moss_event = MossEvent {
            timestamp: Utc::now().to_rfc3339(),
            level: "debug".to_string(),
            message: format!(
                "Stone load: CPU {:.1}%, Memory {:.1}%, Disk {:.1}%",
                cpu_percent, memory_percent, disk_percent
            ),
            job_id: None,
        };
        
        let _ = state.event_tx.send(moss_event);
        
        // TODO: When EventBus is available:
        // let event = DomainEvent::Stone(StoneEvent::LoadUpdated {
        //     stone_name: state.stone_name.clone(),
        //     cpu_percent,
        //     memory_percent,
        //     disk_percent,
        //     timestamp: Utc::now(),
        // });
        // state.event_bus.publish(event).await?;
    }
}

/// Run health monitor task (every 30s)
/// 
/// Computes stone health from metrics and emits StoneEvent::HealthChanged.
pub async fn run_health_monitor_task(_state: AppState) {
    let mut interval = interval(Duration::from_secs(30));
    let mut last_health = "thriving".to_string();
    
    loop {
        interval.tick().await;
        
        // TODO: Get real metrics
        let cpu = 25.0;
        let memory = 45.0;
        
        let new_health = if cpu > 95.0 || memory > 95.0 {
            "wilting"
        } else if cpu > 80.0 || memory > 80.0 {
            "withering"
        } else {
            "thriving"
        };
        
        if new_health != last_health {
            tracing::info!(
                old = %last_health,
                new = %new_health,
                "Stone health changed"
            );
            
            // TODO: Emit StoneEvent::HealthChanged to EventBus
            
            last_health = new_health.to_string();
        }
    }
}
