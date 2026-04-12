use crate::AppState;
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

    // Resolve exact offering name (case-insensitive lookup)
    let resolved_name = state
        .offerings
        .with_active(|offerings| {
            offerings
                .iter()
                .find(|o| o.name.to_string().eq_ignore_ascii_case(offering_name))
                .map(|o| o.name.to_string())
        })
        .await;
    let resolved_name = resolved_name.ok_or_else(|| {
        anyhow::anyhow!(
            "Offering '{}' not found while updating capability set",
            offering_name
        )
    })?;

    // Mutate via gateway (detail-only, no chirp sync)
    let changed = state
        .offerings
        .update_by_name(&resolved_name, |offering| {
            let entry_index = offering
                .sub_capabilities
                .iter()
                .position(|entry| entry.cap_type.eq_ignore_ascii_case(&cap_type));

            let mutated = match (add, entry_index) {
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
                        true
                    } else {
                        false
                    }
                }
                (true, None) => {
                    offering.sub_capabilities.push(SubCapability::new(
                        cap_type.clone(),
                        vec![capability.clone()],
                    ));
                    true
                }
                (false, Some(index)) => {
                    let entry = &mut offering.sub_capabilities[index];
                    let before = entry.items.len();
                    entry
                        .items
                        .retain(|item| !item.eq_ignore_ascii_case(&capability));
                    let became_empty = entry.items.is_empty();
                    let did_change = before != entry.items.len();
                    if became_empty {
                        offering.sub_capabilities.remove(index);
                    }
                    did_change
                }
                (false, None) => false,
            };

            if mutated {
                offering.touch();
            }
            false // sub_capabilities are detail-only, don't trigger chirp sync
        })
        .await;

    let _ = changed; // gateway auto-persists

    Ok(())
}
