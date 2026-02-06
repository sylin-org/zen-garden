use crate::AppState;
use anyhow::Context;
use chrono::Utc;
use garden_common::SubCapability;

pub async fn record_capability_added(
    state: &AppState,
    offering_name: &str,
    cap_type: &str,
    capability: &str,
) -> anyhow::Result<()> {
    mutate_capability_set(state, offering_name, cap_type, capability, true).await
}

pub async fn record_capability_removed(
    state: &AppState,
    offering_name: &str,
    cap_type: &str,
    capability: &str,
) -> anyhow::Result<()> {
    mutate_capability_set(state, offering_name, cap_type, capability, false).await
}

async fn mutate_capability_set(
    state: &AppState,
    offering_name: &str,
    cap_type: &str,
    capability: &str,
    add: bool,
) -> anyhow::Result<()> {
    let cap_type = cap_type.trim().to_ascii_lowercase();
    let capability = capability.trim().to_string();
    if cap_type.is_empty() || capability.is_empty() {
        return Ok(());
    }

    let mut changed = false;
    {
        let mut offerings = state.offerings.write().await;
        let Some(offering) = offerings
            .iter_mut()
            .find(|offering| offering.name.eq_ignore_ascii_case(offering_name))
        else {
            return Err(anyhow::anyhow!(
                "Offering '{}' not found while updating capability set",
                offering_name
            ));
        };

        let entry_index = offering
            .sub_capabilities
            .iter()
            .position(|entry| entry.cap_type.eq_ignore_ascii_case(&cap_type));

        match (add, entry_index) {
            (true, Some(index)) => {
                let entry = &mut offering.sub_capabilities[index];
                if !entry
                    .items
                    .iter()
                    .any(|existing| existing.eq_ignore_ascii_case(&capability))
                {
                    entry.items.push(capability.clone());
                    entry.items.sort_by_key(|item| item.to_ascii_lowercase());
                    entry.items.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
                    entry.discovered_at = Some(Utc::now());
                    changed = true;
                }
            }
            (true, None) => {
                offering.sub_capabilities.push(SubCapability::new(
                    cap_type.clone(),
                    vec![capability.clone()],
                ));
                changed = true;
            }
            (false, Some(index)) => {
                let entry = &mut offering.sub_capabilities[index];
                let before = entry.items.len();
                entry
                    .items
                    .retain(|item| !item.eq_ignore_ascii_case(&capability));
                let became_empty = entry.items.is_empty();
                if before != entry.items.len() {
                    changed = true;
                }
                if became_empty {
                    offering.sub_capabilities.remove(index);
                }
            }
            (false, None) => {}
        }

        if changed {
            offering.touch();
        }
    }

    if changed {
        state
            .persist_offerings()
            .await
            .context("Failed to persist offerings after capability mutation")?;
    }

    Ok(())
}
