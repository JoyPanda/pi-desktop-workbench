# Vendored gpui-pre-windows

Source: crates.io `gpui-pre-windows` 0.3.3
(sha256 `1a4b60579aa2cb4ef7b6f22133a7b9fe12458062961343bdcf36e259047bc7ad`).
It is Zed's `gpui_windows` crate re-published from Zed commit
`5b055fa789a8b8d38ac951a6e0cde272f66b4495`.

This tree keeps `src/`, `build.rs`, `Cargo.toml`, `Cargo.toml.orig`, and
`LICENSE-APACHE`. The crate has no examples or integration tests, so no targets
were dropped.

## Version lock — read before touching any version

The whole gpui-pre family is pinned to `=0.3.3` exactly (workspace
`[patch.crates-io]`, `crates/orbit-pi/Cargo.toml`, and the `=0.3.3` pins inside
this tree). **0.3.14 has deleted the ClearType/subpixel rasterizer** this crate
still carries (`color_text_raster.hlsl`, the `subpixel_sprite` pipeline, and
`DWRITE_TEXTURE_CLEARTYPE_3x1` in `direct_write.rs`). Semver says 0.3.3 → 0.3.x
is compatible; it is not. A bump is its own migration project, never a routine
`cargo update`.

## Local hunks

1. **`src/directx_renderer.rs`, `DirectXRenderer::render` — no opaque white
   clear.** Upstream clears the render target to `[1.0; 4]` for
   `WindowBackgroundAppearance::Opaque` and to transparent otherwise. Every
   clear is transparent here. The scene covers the window in the same frame, and
   a white clear flashes on a dark theme during resize, on the first frame, and
   during device-lost recovery. This mirrors the vendored gpui 0.2.2 `pre_draw`
   patch (see `third_party/gpui/UPSTREAM.md`). The `background_appearance`
   parameter stays in the signature because `PlatformWindow::draw` and the
   test-support `render_to_image` both pass it.

Nothing else is forked. ClearType comes from upstream unchanged: the shaders,
the `SubpixelSprite` batch, and the DirectWrite rasterization are byte-identical
to 0.3.3 and must not be re-patched here.
