# AGENTS: Sp9ceVerb

Project profile for agent work in this repository.

## Product

Sp9ceVerb, a reverb plugin with nine spaces on one engine. CLAP id
`com.opengrid.sp9ceverb`, vendor OpenGrid (source MIT, official binaries freeware; copyright Marcos Quirino Deida).

## Architecture

- `crates/engine`: the DSP (`sp9ce-engine`). `params.rs` is the single source
  of truth for parameter names, ranges and defaults; `engine.rs` is the signal
  path. `preset.rs` parses the text files in `presets/`. `forbid(unsafe_code)`,
  no dependencies.
- `plugin`: `plugin.rs` implements `openplugin::Plugin` and the lock-free state
  shared with the editor. `gui/` is the editor (layout and input in `mod.rs`,
  the three displays in `visual.rs`, controls in `widgets.rs`, Core Text
  lettering in `text.rs`). `cocoa.rs` is the `NSView`. `clap_host.rs` exports
  `clap_entry`: the SDK's CLAP adapter does the audio side, and four hooks on
  its plugin object add the `clap.gui` extension and carry editor gestures.
  `presets.rs` holds the files of `presets/`, embedded by `build.rs`; the
  editor's preset bar and browser load them as ordinary parameter gestures.
  `shell_host.rs` is the C ABI the SDK's VST3, AU and AAX shells link:
  `op_shell_*` (audio, forwarded to the SDK), `op_shell_tail_seconds`, and
  `op_editor_*` (the same editor, gestures sent to the host on the main thread).

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
- The reported tail (`plugin::tail_seconds`, one function for every format)
  must cover what the engine really rings: the low-band decay, the plate and
  chamber echoes, and infinity for Infinite and for Chaos above zero. Change
  it together with anything that lengthens the ring-out.
- Never rebuild, reinstall or re-sign while a host suite or validator is
  running: it tests the installed bundles. Use `--check-only` for the editor
  checks.
- Who does what: the agent that writes the plugin code does not run plugin
  QA, and the agent that runs the tests and QA does not edit plugin code,
  the SDK shells or the build scripts.
- No commits without the user's approval.

## Build and test

```sh
cargo test --workspace --release
scripts/build.sh [--install]
scripts/build_shells.sh [--install] [--check]   # VST3, AU, AAX; --check clicks each editor's preset arrow
scripts/build_shells.sh --check-only            # the same checks on what is already built; builds nothing
auval -v aufx Sp9v OpGd
scripts/sign_aax.sh [--install]         # settings in scripts/signing.local.env (untracked)
cd plugin && . ../scripts/sdk_env.sh && cargo_sdk test --release
clap-validator validate dist/Sp9ceVerb.clap
```

Host test: the plugin-daw-test skill, `--fx "CLAP: Sp9ceVerb"`. Voicing controls
only act in their own voicing, so the exceptions file gives each one a
`@ Mode=<index/8>` context (Chaos details also need `@ Chaos=0.9`).

Measurement: `cargo build --release -p sp9ce-engine --example probe`, then
`python3 tools/measure.py <tag> <mode> <label> [key=value ...]`.
