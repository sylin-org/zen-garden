//! Inter-stone HTTP client trait for pond operations.

use reqwest::RequestBuilder;

/// Stone-to-stone HTTP client abstraction.
///
/// The security domain uses this to communicate with peer stones
/// without depending on the concrete `StoneClient` from infra.
pub trait PondClient: Send + Sync {
    /// Build a GET request to the given peer address and path.
    fn get(&self, address: &garden_common::PeerAddress, path: &str) -> RequestBuilder;

    /// Build a POST request to the given peer address and path.
    fn post(&self, address: &garden_common::PeerAddress, path: &str) -> RequestBuilder;

    /// Build a PUT request to the given peer address and path.
    fn put(&self, address: &garden_common::PeerAddress, path: &str) -> RequestBuilder;

    /// Build a DELETE request to the given peer address and path.
    fn delete(&self, address: &garden_common::PeerAddress, path: &str) -> RequestBuilder;

    /// Reload TLS configuration after enrollment changes.
    fn reload_tls(&self);
}
