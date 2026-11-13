//! HTTPS inside the app: `ureq` with rustls, checking certificates against the Mozilla
//! roots built in by `webpki-roots`, so the device needs no certificate files.

use std::time::Duration;
use ureq::config::ConfigBuilder;
use ureq::typestate::AgentScope;

/// What every download here starts from: HTTPS only, 15 seconds to connect and `whole` for
/// the request from start to end.
pub(crate) fn config(whole: Duration) -> ConfigBuilder<AgentScope> {
    ureq::Agent::config_builder()
        .https_only(true)
        .timeout_connect(Some(Duration::from_secs(15)))
        .timeout_global(Some(whole))
}
