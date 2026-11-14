# AGENTS: Sp9ceVerb

Project profile for agent work in this repository.

## Product

Sp9ceVerb, a reverb plugin with nine spaces on one engine. CLAP id
`com.opengrid.sp9ceverb`, vendor OpenGrid (source MIT, official binaries freeware; copyright Marcos Quirino Deida).

## Architecture

- `crates/engine`: the DSP (`sp9ce-engine`). `params.rs` is the single source
  of truth for parameter names, ranges and defaults; `engine.rs` is the signal
  path. `forbid(unsafe_code)`, no dependencies.
- `plugin`: `plugin.rs` implements `openplugin::Plugin` and the lock-free state
  shared with the editor. `gui/` is the editor (layout and input in `mod.rs`,
  the three displays in `visual.rs`, controls in `widgets.rs`, Core Text
  lettering in `text.rs`). `cocoa.rs` is the `NSView`. `clap_host.rs` exports
  `clap_entry`: the SDK's CLAP adapter does the audio side, and four hooks on
  its plugin object add the `clap.gui` extension and carry editor gestures.

## Constraints

- Realtime-safe engine: no allocation, no locks, no panics on the process path
  (`crates/engine/tests/no_alloc.rs`).
- Adding a voicing must leave the others bit-identical unless that is the point
  of the change; the Chaos path is skipped entirely outside its own voicing.
- Chaos output stays at or below -0.5 dBFS and is deterministic.
- No machine paths in the repository or the binaries: dependencies are located
  by `scripts/sdk_env.sh`, and `scripts/build.sh` remaps build paths.
- Do not copy code, tables or presets from other reverbs. Read
  `docs/PROVENANCE.md` before changing the engine.
- No commits without the user's approval.

## Build and test

```sh
cargo test --workspace --release
scripts/build.sh [--install]
cd plugin && . ../scripts/sdk_env.sh && cargo_sdk test --release
clap-validator validate dist/Sp9ceVerb.clap
```

Host test: the plugin-daw-test skill, `--fx "CLAP: Sp9ceVerb"`. Voicing controls
only act in their own voicing, so the exceptions file gives each one a
`@ Mode=<index/8>` context (Chaos details also need `@ Chaos=0.9`).

Measurement: `cargo build --release -p sp9ce-engine --example probe`, then
`python3 tools/measure.py <tag> <mode> <label> [key=value ...]`.
