//! Local fork release channels.
//!
//! Keep fork-specific repository and signing choices here so upstream merges
//! only need to preserve one small module plus a few call sites.

pub const IS_FORK: bool = true;
pub const APP_REPOSITORY: &str = "weiang12345/agent2api";
pub const PLUGIN_REPOSITORY: &str = "weiang12345/Agent2Providers";
pub const PROVIDERS_PACK_ID: &str = "agent2api-providers";

pub const PLUGIN_SIGNING_KEY_ID: &str = "fork-2026";
pub const PLUGIN_SIGNING_PUBLIC_KEY: &str =
    "7a0f200950e1f87c7eced5e6d8465de7b77f880100fe509c054b36dca7cbcd55";

pub fn app_repository<'a>(indexed: &'a str) -> &'a str {
    if IS_FORK {
        APP_REPOSITORY
    } else {
        indexed
    }
}

pub fn plugin_repository<'a>(pack_id: &str, indexed: &'a str) -> &'a str {
    if IS_FORK && pack_id == PROVIDERS_PACK_ID {
        PLUGIN_REPOSITORY
    } else {
        indexed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fork_providers_pack_uses_fork_repository() {
        assert_eq!(
            plugin_repository(
                PROVIDERS_PACK_ID,
                "Agent2Builder/agent2api-providers-releases"
            ),
            PLUGIN_REPOSITORY
        );
    }

    #[test]
    fn unrelated_packs_keep_their_indexed_repository() {
        assert_eq!(
            plugin_repository(
                "agent2api-web-providers",
                "Agent2Builder/agent2api-providers-releases"
            ),
            "Agent2Builder/agent2api-providers-releases"
        );
    }
}
