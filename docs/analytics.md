# Analytics / Telemetry

Orbit sends a small, privacy-first, opt-out stream of product telemetry to a
self-hosted [Countly](https://countly.com/) instance. All telemetry lives in the
`crates/orbit-analytics` crate; the app only ever sees the `Analytics` trait, so
Countly can be replaced without touching call sites.

```
Orbit (GPUI) → crate::analytics → orbit-analytics (queue + worker) → HTTPS → Countly
```

Nothing in this path blocks the UI: `track` pushes onto a bounded in-memory
queue and wakes a dedicated worker thread that batches and sends requests.

## What is collected

* A **pseudonymous installation id** — a random v4 UUID generated on first run.
* A **session id** — a new random UUID per launch.
* `app_version`, `os`, `architecture`, `locale`, and `environment`.
* A small, closed set of feature events (see below). Event-specific properties
  are limited to coarse enum values such as `reason=provider` or
  `mode=dark`; no values or free-form text.

### Events

| Event | When |
| --- | --- |
| `app_started`, `app_closed` | application lifecycle |
| `session_started`, `session_ended` | Countly session boundary |
| `project_created`, `project_deleted` | a folder is added to / removed from the sidebar |
| `project_opened` | a workspace/session becomes active |
| `terminal_opened`, `terminal_session_created` | the bottom terminal panel / its shell |
| `agent_started`, `agent_completed`, `agent_failed` | a pi run starts, settles, or fails |
| `mcp_connected`, `mcp_disconnected`, `mcp_connection_failed` | MCP server status transitions |
| `command_palette_opened` | ⌘P/⌘K palette opens |
| `settings_opened`, `settings_changed` | Settings surface / a preference changes |
| `theme_changed` | the appearance palette or mode changes |
| `update_available`, `update_installed` | the signed updater |

There is deliberately no click/keypress/mouse/page-view telemetry, no session
replay, and no screen, terminal, or conversation recording.

## What is never collected

Source code; project names, paths, or contents; repository names/URLs; file
names/contents; terminal commands or output; shell history; environment
variables; API keys, access tokens, passwords, or MCP credentials; auth
cookies; clipboard contents; agent prompts or responses; conversation contents;
private URLs; hostnames, MAC addresses, serial numbers, or disk identifiers; IP
address as an analytics property; exact location.

Enforcement is structural: every event is a variant of the closed
`AnalyticsEvent` enum and its properties are built explicitly. In addition,
`orbit_analytics::privacy` defines a forbidden-key list and strips/detects
sensitive keys and value shapes as defense in depth. Tests assert that no event
carries a forbidden property and that a built request contains no paths, emails,
URLs, or secret-looking values.

## Identity

* On first run `orbit-analytics` generates a random UUID v4 and persists it to
  `~/.orbit-pi/analytics.json`. It is **pseudonymous**, not legally anonymous:
  it is stable across launches so Countly can count unique installations.
* It is never derived from an email, username, hostname, MAC address, CPU/disk
  serial, or hardware UUID.
* A new session id is generated every launch; the installation id is never used
  as a session id.
* `AnalyticsClient::reset_identity()` generates a new installation id and
  session id and clears pending events. (No UI exposes it today.)

## Adding an event

1. Add a variant to `AnalyticsEvent` in `crates/orbit-analytics/src/event.rs`.
2. Add its `name()` arm and, only if genuinely useful, a scalar property in
   `properties()`. Add the variant to `AnalyticsEvent::ALL`.
3. Call `crate::analytics::track(cx, orbit_analytics::AnalyticsEvent::YourEvent)`
   at the right place in the app. Never add a second analytics path or call
   Countly directly.

Keep properties coarse and from a closed set. Never pass a struct, path, string
from user content, or `serde_json::to_value(some_app_state)`.

## Countly API

Orbit talks to Countly's documented `/i` HTTP endpoint (form-encoded POST):

* `app_key`, `device_id` (the installation id), `session_id`, `timestamp`
* `begin_session=1` / `end_session=1` + `session_duration` for sessions
* `metrics` = `{"_os", "_app_version", "_device_type": "desktop", "_locale"}`
* `events` = JSON array of `{ "key", "count", "timestamp", "segmentation" }`
  where `segmentation` always carries `app_version`, `os`, `architecture`,
  `environment`, and optional `locale`.

Multiple events are batched into one request (default batch size 10). TLS
verification is never disabled and only HTTPS endpoints are accepted (loopback
HTTP is allowed solely so the integration test's mock server works).

## Configuration

`AnalyticsConfig` carries the endpoint, app key, environment, batching/timeout
knobs, and metadata. `config_from_env()` resolves it from the process
environment:

| Variable | Meaning |
| --- | --- |
| `ORBIT_ANALYTICS_APP_KEY` | Countly application key. **Required to send.** |
| `ORBIT_ANALYTICS_ENDPOINT` | Base URL; default `https://telemetry.rajeshwar.tech`. |
| `ORBIT_ANALYTICS_ENVIRONMENT` | `development` / `staging` / `production`. |
| `ORBIT_ANALYTICS_DISABLED` | truthy disables sending regardless of other settings. |

Development and staging are **off by default** so a `cargo run` never pollutes
the production dashboard. Without an app key the client is inert and Orbit runs
normally. The app key is configuration, not a committed secret.

Release builds embed the key at compile time: `crates/orbit-analytics/build.rs`
bakes the build-time `ORBIT_ANALYTICS_APP_KEY` into the binary, and the release
workflow passes the `ORBIT_ANALYTICS_APP_KEY` GitHub secret to every platform
build. A process-environment `ORBIT_ANALYTICS_APP_KEY` still overrides the baked
value at launch, which is how local development points at a local endpoint.

### Changing the endpoint

Pass a different `AnalyticsConfig` to `AnalyticsClient::new`, or set
`ORBIT_ANALYTICS_ENDPOINT`. The endpoint must be HTTPS (or loopback). Because
the app reads the global, no application code changes are needed.

## Disabling telemetry

Settings → **Privacy → Anonymous Usage Analytics**. Turning it off:

1. stops new events from being collected,
2. clears the in-memory queue (and any pending batch),
3. stops background requests,
4. persists the choice to `~/.orbit-pi/analytics.json`.

Re-enabling resumes collection; nothing queued while disabled is ever sent
later. `ORBIT_ANALYTICS_DISABLED=1` forces it off for a process.

## Offline behavior and retries

Network failures are non-fatal and never surface to the user. Retryable errors
(timeouts, 408/429/5xx, DNS/TLS failures) are retried with exponential backoff
(base 2s, capped at 60s) up to 5 consecutive attempts; permanent errors (e.g.
400/401/403) drop the batch immediately. The queue is hard-capped (default 200
events, oldest dropped first), so telemetry storage can never grow unbounded.

## Testing locally

The crate is fully testable without the production server:

```
cargo test -p orbit-analytics
```

* Unit tests drive the queue/session engine with an in-memory transport, cover
  identity persistence/reset, opt-out clearing, event naming/properties, and
  privacy filtering.
* `tests/countly_http.rs` starts a `TcpListener` mock server, sends a real
  request through `CountlyTransport`, and asserts the request line, form fields,
  event array, metadata, and the absence of prohibited data.

To exercise the real HTTP path by hand, point the endpoint at a local mock:

```
ORBIT_ANALYTICS_APP_KEY=test \
ORBIT_ANALYTICS_ENDPOINT=http://127.0.0.1:9999 \
ORBIT_ANALYTICS_ENVIRONMENT=development \
cargo run -p orbit-pi
```

(Development is disabled by default; the key plus the loopback endpoint enables
the client for the current process.)

## Countly server setup

The production instance is `https://telemetry.rajeshwar.tech`, fronted by the
existing reverse proxy. Configure a dedicated Countly application:

* Name: **Orbit**, Platform: **Desktop**, Environment: **Production**.
* Provide its application key to the client via `ORBIT_ANALYTICS_APP_KEY` (build
  or launch environment). Do not commit it to source.
* MongoDB must stay on the private network and never be publicly exposed; only
  the Countly HTTP endpoint is published.

Verify the endpoint without the app:

```
curl -i https://telemetry.rajeshwar.tech/i
# => 400 {"result":"Missing parameter \"app_key\" or \"device_id\""} when Countly is up
curl -i "https://telemetry.rajeshwar.tech/i?app_key=YOUR_KEY&device_id=test"
# => 200 {"result":"Success"} once the app exists
```

See `PRIVACY.md` for the user-facing statement.
