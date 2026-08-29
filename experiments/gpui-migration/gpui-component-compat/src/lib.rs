//! Isolated compatibility proof for Butter Paper's reviewed GPUI Component stack.
//!
//! This crate must remain independent from the product gallery until its exact
//! source, license, build, and native-runtime gates pass.

mod accessible_button;
pub mod adaptive_performance;
pub mod application_close;
pub mod application_close_workspace;
pub mod cad_view_control;
pub mod continuous_view_control;
pub mod document_resource;
pub mod document_session;
pub mod document_tab_bar;
mod document_viewer;
pub mod document_workspace;
pub mod dimension_property_inspector;
pub mod ink_property_inspector;
pub mod engineering_visual_property_inspector;
pub mod local_signature;
pub mod measurement_property_inspector;
pub mod native_application;
pub mod native_document_view_state;
pub mod native_launch;
pub mod native_runtime_layout;
pub mod page_scale_control;
pub mod page_view_control;
pub mod perf_capture_signal;
pub mod perf_protocol;
pub mod perf_scenario;
pub mod rectangle_property_inspector;
pub mod session_manifest;
pub mod straight_line_property_inspector;
pub mod vertex_path_property_inspector;
pub mod system_theme;
pub mod template_manager;
pub mod text_box_property_inspector;
mod viewer_icons;
pub mod viewer_toolbar_strip;
pub mod zoom_control;
