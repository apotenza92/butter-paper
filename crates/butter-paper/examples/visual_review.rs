//! Development tool: renders the document workspace off-screen with real
//! Metal, replays an input script and saves screenshots for visual review.
//! macOS only; see `visual_review_macos/review.rs` for the script format.
//!
//! cargo run --example visual_review -- <script> <output-dir>

#[cfg(target_os = "macos")]
#[path = "visual_review_macos/review.rs"]
mod review;

fn main() {
    #[cfg(target_os = "macos")]
    review::main();
    #[cfg(not(target_os = "macos"))]
    eprintln!("visual_review runs on macOS only");
}
