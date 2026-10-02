//! Apache-2.0 replacement for the GPL-marked Zed tracing facade.
//!
//! The reviewed GPUI graph uses only the `instrument` attribute. Re-exporting
//! the upstream `tracing` attribute preserves that narrow public seam without
//! copying code from `ztracing`, `ztracing_macro`, or `zlog`.

pub use tracing::instrument;
