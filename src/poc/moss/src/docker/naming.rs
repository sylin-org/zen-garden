use anyhow::Result;
use garden_common::constants::{OFFERING_CONTAINER_PREFIX, OFFERING_FQN_CONTAINER_SEPARATOR};
use garden_common::offerings::OfferingFqn;

pub fn zen_offering_container_name(offering_name: &str) -> Result<String> {
    let fqn = OfferingFqn::parse(offering_name)
        .map_err(|e| anyhow::anyhow!("Invalid offering name '{}': {}", offering_name, e))?;
    Ok(format!(
        "{}{}",
        OFFERING_CONTAINER_PREFIX,
        fqn.encoded_for_container()
    ))
}

pub fn decode_zen_offering_container_name(container_name: &str) -> Option<String> {
    let trimmed = container_name.trim_start_matches('/');
    let suffix = trimmed.strip_prefix(OFFERING_CONTAINER_PREFIX)?;
    Some(decode_offering_container_suffix(suffix))
}

fn decode_offering_container_suffix(encoded: &str) -> String {
    // Image-direct containers: img-nginx-latest -> image:nginx-latest (best-effort)
    if let Some(rest) = encoded.strip_prefix("img-") {
        if let Some((sanitized_ref, instance)) = rest.split_once(OFFERING_FQN_CONTAINER_SEPARATOR) {
            return format!("image:{}::{}", sanitized_ref, instance);
        }
        return format!("image:{}", rest);
    }

    // Curated containers: mongodb--prod -> mongodb::prod
    if let Some((offering, instance)) = encoded.split_once(OFFERING_FQN_CONTAINER_SEPARATOR) {
        format!("{}::{}", offering, instance)
    } else {
        encoded.to_string()
    }
}
