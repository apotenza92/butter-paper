//! Isolated compatibility proof for Butter Paper's reviewed GPUI Component stack.
//!
//! This crate must remain independent from the product gallery until its exact
//! source, license, build, and native-runtime gates pass.

pub mod application_close;
pub mod application_close_workspace;
pub mod cad_view_control;
pub mod continuous_view_control;
pub mod document_tab_bar;
mod document_viewer;
pub mod document_workspace;
pub mod native_application;
pub mod native_document_view_state;
pub mod native_launch;
pub mod page_scale_control;
pub mod page_view_control;
pub mod perf_capture_signal;
pub mod perf_protocol;
pub mod perf_scenario;
pub mod rectangle_property_inspector;
pub mod viewer_toolbar_strip;
pub mod zoom_control;
