// SPDX-License-Identifier: GPL-3.0-or-later

#![doc = include_str!("../README.md")]

/// Placeholder entry point: the application itself starts in roadmap phase 1.
///
/// For now it only states what it is, which is enough to prove that the build
/// and packaging pipeline works from end to end on both systems.
#[allow(
    clippy::print_stdout,
    reason = "placeholder output until the window exists in phase 1"
)]
fn main() {
    println!(
        "{} {} - pre-alpha foundations. There is nothing to play with yet.",
        daw_ident::PRODUCT_NAME,
        env!("CARGO_PKG_VERSION"),
    );
}
