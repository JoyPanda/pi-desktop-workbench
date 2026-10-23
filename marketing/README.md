# Orbit — marketing site

The landing page for Orbit, the native GPUI workbench for the pi coding agent.
Built with Next.js (App Router), Tailwind CSS v4, and Motion.

## Develop

```bash
npm install
npm run dev      # http://localhost:3000
```

## Build

```bash
npm run build
npm run start
```

## Structure

```
src/app/layout.tsx          fonts (Inter + Geist Mono) + metadata
src/app/globals.css         reference palette + shell/band/dash utilities
src/app/page.tsx            section order
src/app/changelog/page.tsx  release feed (repo CHANGELOG.md + GitHub releases)
src/app/changelog/[version]/  per-release page + its Open Graph image
src/components/             nav, footer, ui primitives, screenshot frame
src/components/download-button.tsx  OS-aware download CTA + options menu
src/components/sections/    hero, stage, steps, workbench, features,
                            guard, review, gallery, closer
src/components/changelog/   release entries, inline markdown, skeleton
src/components/ui/          shadcn primitives (button, card, badge,
                            skeleton, empty)
src/lib/changelog.ts        changelog + release parsing and formatting
src/lib/downloads.ts        release assets → per-platform download lists
public/screens/             real app screenshots (window-only)
```

The copy describes the **app** — its features (chat, tools, sessions, find,
providers, plugins, models, usage, appearance) and the steps you take in it
(connect a provider → start a task → watch it work → review → commit), not the
repository's engineering internals.

Screenshots in `public/screens/` are the real app, captured window-only
(rounded corners and shadow, no desktop or menu bar). They are `next/image`-
optimized at runtime; the hero shot is `priority`, the rest lazy-load.

The visual system mirrors the warm.run landing: a 1280px `shell` with hairline
rules down each edge, dashed separators between `band`s, a dark slate page
(`#141518`) with recessed well bands, `#1c1d21` cards, a single blue micro-accent
(`#5b9dff`), and Inter at light weights (250/300/350) with Geist Mono for labels.

Dark is the default. Every colour resolves through CSS variables defined in
`:root` (dark) and `.light`, so a nav toggle can flip the whole site; the
choice is stored in `localStorage` and applied before paint by an inline
script in the root layout.
