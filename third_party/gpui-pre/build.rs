#![allow(clippy::disallowed_methods, reason = "build scripts are exempt")]

// Orbit fork: the upstream script embeds `resources/windows/gpui.manifest.xml`
// into every Windows binary whenever the `windows-manifest` feature is on, and
// `gpui-pre-platform` turns that feature on unconditionally. Orbit embeds its
// own `crates/orbit-pi/resources/windows.manifest` from the app's build script,
// and two `RT_MANIFEST` resources of the same type/id/language make CVTRES fail
// the link with `CVT1100: duplicate resource`. So the embed is a no-op here and
// the app's manifest — the superset, with per-monitor-v2 DPI, long paths,
// `asInvoker`, and the supportedOS list — is the one that ships.

fn main() {
    println!("cargo::rustc-check-cfg=cfg(gles)");
}
