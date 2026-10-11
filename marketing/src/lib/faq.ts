/**
 * Questions that come up in the pi community. Shared by the FAQ section and its
 * `FAQPage` structured data, so the visible copy and the schema never drift.
 */
export const FAQ_ITEMS = [
  {
    q: "Is Orbit free?",
    a: "Yes. Orbit is free and open source under the Apache-2.0 license — no account, no subscription, and no paywalled features. You bring your own model providers.",
  },
  {
    q: "Does Orbit replace pi or bundle it?",
    a: "Neither. Orbit is a native desktop client that drives the pi CLI you already have installed over pi's RPC interface, using your existing configuration, sessions, providers, tools, and extensions. The terminal and the app stay in sync.",
  },
  {
    q: "Which platforms does Orbit support?",
    a: "macOS 13+, Windows, and Linux. Orbit is a native Rust and GPUI application — no Electron and no web view — so it starts fast and stays lightweight.",
  },
  {
    q: "How is Orbit different from the pi TUI or pi-web?",
    a: "The pi TUI lives in the terminal and pi-web wraps pi in a browser. Orbit is a native desktop workbench: multiple sessions and workspaces, an inline diff review pane, Git and GitHub, usage analytics, MCP, and plugins in one window.",
  },
  {
    q: "Does Orbit work with my pi extensions and plugins?",
    a: "Yes. Orbit talks to pi over RPC, so agent-side pi extensions keep working and Orbit renders events like questions and approvals natively. Plugins installs, updates, and removes pi packages too.",
  },
  {
    q: "Where does my data go? Does Orbit send telemetry?",
    a: "Orbit is local-first: your code and sessions stay on your machine. Anonymous, provider-independent product analytics are documented in the privacy policy and can be turned off.",
  },
  {
    q: "Can I run multiple pi sessions at once?",
    a: "Yes. Orbit is built for concurrent sessions and workspaces, each backed by your local pi installation, so you can run several agents without juggling terminal windows.",
  },
] as const;

export type FaqItem = (typeof FAQ_ITEMS)[number];
