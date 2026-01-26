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
        
        // TODO: Real system metrics (sysinfo crate?)
        let cpu_percent = 25.0;
        let memory_percent = 45.0;
        let disk_percent = 60.0;
        
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
