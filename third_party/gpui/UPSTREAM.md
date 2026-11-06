# Vendored gpui

Source: crates.io `gpui` 0.2.2, Zed commit `69e2130295c2649963eb639fc70b4f2ee8ea1624`.

This tree keeps `src/`, `resources/`, `build.rs`, `Cargo.toml`, `LICENSE-APACHE`, and `README.md`. Examples and integration tests from the published crate are not included.

## ClearType patch

Windows ClearType comes from Zed PR 45423 (merge `47d2694dcceae02ff9ffd5d7d174389dde6a7bf3`). Only `crates/gpui` was ported. Zed settings, the settings UI, and `crates/zed` were not.

Linux stays on the 0.2.2 cosmic-text rasterizer and on grayscale. `blade-graphics` 0.7 has no dual-source blend, so it does not create a subpixel pipeline, and `is_subpixel_rendering_supported` returns false. macOS stays on Core Text grayscale.

Window clears stay transparent on Windows and Linux. Do not copy the upstream opaque-window white clear; Orbit's theme is dark and that clear flashes.
