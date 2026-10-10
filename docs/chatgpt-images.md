# ChatGPT Images (experimental)

PhotoCraft's native editor can ask the official local Codex CLI to generate one image using
an existing ChatGPT subscription. Open **Window → ChatGPT Images**, enter a description,
and select **Check connection**. **Generate image** is enabled only after the account and
included allowance checks pass. A successful result opens as a new unsaved, editable document;
existing documents stay intact. The normal Save As and Export commands save the result.
Generation runs as a cancellable background job. Images consume more included allowance
than ordinary Codex turns; this is not unlimited or free outside your subscription.

## Requirements and boundaries

- Native desktop or an explicit trusted local CLI invocation. The browser/WASM build is disabled.
- Official Codex CLI **0.162.0**, with its existing native image-generation feature enabled.
  Other versions are disabled until their image protocol is checked. The app does not enable flags.
- Existing ChatGPT login in the CLI and available included usage. API-key accounts, unknown
  allowance, exhausted windows, available/unknown extra credits, and unavailable native image
  generation fail closed.
- No API key, cookie/session scraping, custom HTTP endpoint, token exchange, or login flow.
  PhotoCraft never reads or copies credential files. Codex manages its existing authentication.
- No paid API fallback and no purchasing/redeeming/requesting extra credits. Codex can draw
  from existing credits after included limits, so preflight also requires a known unavailable
  credit balance. PhotoCraft cannot atomically reserve allowance or impose an account billing
  cap; concurrent usage or account changes during a turn remain a service limitation.
- Local/open-source experimental use only. Official app-server authentication is not permitted
  for commercial or hosted services; this feature must not be turned into a hosted gateway.
- One PNG, at most 16 MiB and 4096 pixels on either edge. Exact dimensions, image edits,
  references, batch generation, and pixel/selection replacement are not offered in this first version.
- The backend may take several minutes or fail. Quota can change between a check and a turn.
  A quota/authorization/protocol failure reports an error and never inserts a placeholder image.

Install/sign in with the official CLI separately. PhotoCraft does not install it or initiate a
new persistent authorization. `PHOTOCRAFT_CODEX_BIN` may identify an absolute local official
binary path; it is host configuration, never an automation parameter. PATH binaries and the
official npm Windows binary are discovered automatically.

## Architecture and verified contract

The existing product is Rust/egui/eframe on wgpu. Before this change it had no generative
backend. `photocraft-imagegen` is an L4 provider seam with a native stdio adapter;
`engine::imagegen_cmds` exposes `imagegen.status` and `imagegen.generate` through existing
background jobs. The dialog's prompt and window state use the ordinary serializable fields.
Untrusted MCP/control sessions cannot invoke these account-backed commands or nested action steps.

The contract was generated from the installed official binary with `codex app-server generate-ts`:
`ThreadItem` contains `type: "imageGeneration"`; `ImageGenerationItem` has `status`, `result`,
optional `savedPath`, and `failure` (`usageLimitExceeded`). The adapter uses documented
`initialize`, `account/read`, `account/rateLimits/read`, `thread/start`, `turn/start`,
`item/completed`, and `turn/completed` messages. It accepts completed PNG base64/data URLs
or a bounded PNG in Codex's canonical `generated_images` folder. It never fetches image URLs
or accepts a model's plain-text success claim. A fresh empty workspace and ephemeral read-only
thread isolate each request; shell, app, browser, computer, and multi-agent features are disabled
for the child process only, and additional server requests are rejected. Child processes are hidden
on Windows and terminated on cancellation, error, or completion.

The generated types declare `status` and `result` as strings. The adapter's expected
`completed` status and PNG payload encoding are fixture-tested and remain live-unverified.

## Verification on 5900X, 2026-10-10

The official CLI version, enabled image feature, generated protocol types, and existing
ChatGPT Pro account were verified. The native allowance query reported **100% used** for
limit ID `codex`, its `primary` window of **10,080 minutes**, resetting at
**2026-10-14 03:44:45 UTC** (`resetsAt: 1791949485`). No secondary window was returned.
This is a Codex quota result, not a statement about every ChatGPT feature.
The account also reported `hasCredits: true`, `unlimited: false`; the new guard refuses to
generate with available credits, even after the included window resets. A service-enforced
included-only spending control would need verification before relaxing that guard.

The real-image smoke test stopped before sending a prompt. **No real generated image has
been verified or delivered.** Parser, mock provider, document import/edit/undo, failure,
allowance, cancellation, automation, and UI checks are separate from real-service acceptance.
Once included usage and the no-credit preflight both pass, one real image must still confirm
the result payload and save path. No account payment setting was changed during verification.

Local verification passed: 2,294 tests across the four touched crates (22 opt-in tests ignored),
plus the full-registry `panic_hunt`; clippy with warnings denied, dependency layers, WASM,
format, and generated parity/scorecard consistency. Six actual offscreen captures were
inspected: all five themes at 1440x900, plus 760x480. These are UI evidence, not generated art.

Fixture/mock coverage includes incomplete/failed native image items, usage-limit failures,
unsupported image URLs, malformed/non-PNG payloads, invalid/private saved paths, excessive
dimensions, bad prompts, API-key accounts, unknown/exhausted limits, available/unknown credits,
engine generation failure/cancellation, untouched existing 16-bit documents, editable new
documents with undo/redo, disabled UI confirmation, and direct automation-command refusal.
Image-generation transport, successful generation, and the native saved-image payload still need
the one-image acceptance test when the subscription-only preconditions can be satisfied.

Sources: [Codex image generation](https://learn.chatgpt.com/docs/image-generation),
[official app-server protocol and authentication restrictions](https://learn.chatgpt.com/docs/app-server),
[Codex credit behavior](https://help.openai.com/en/articles/12642688-using-credits-for-flexible-usage-in-chatgpt-personal-plans), and
[Sign in with ChatGPT preview limitations](https://developers.openai.com/siwc/token-sharing-open-source/preview-limitations).
The new Sign in with ChatGPT subscription Responses route lists image generation as unsupported;
it is not used by this provider.
