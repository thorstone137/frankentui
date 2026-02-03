#![forbid(unsafe_code)]

//! VOI debug telemetry snapshots for runtime introspection.

use std::sync::{LazyLock, RwLock};

use crate::voi_sampling::VoiSamplerSnapshot;

static INLINE_AUTO_VOI_SNAPSHOT: LazyLock<RwLock<Option<VoiSamplerSnapshot>>> =
    LazyLock::new(|| RwLock::new(None));

/// Store the latest inline-auto VOI snapshot.
pub fn set_inline_auto_voi_snapshot(snapshot: Option<VoiSamplerSnapshot>) {
    if let Ok(mut guard) = INLINE_AUTO_VOI_SNAPSHOT.write() {
        *guard = snapshot;
    }
}

/// Fetch the latest inline-auto VOI snapshot.
#[must_use]
pub fn inline_auto_voi_snapshot() -> Option<VoiSamplerSnapshot> {
    INLINE_AUTO_VOI_SNAPSHOT
        .read()
        .ok()
        .and_then(|guard| guard.clone())
}

/// Clear any stored inline-auto VOI snapshot.
pub fn clear_inline_auto_voi_snapshot() {
    set_inline_auto_voi_snapshot(None);
}
