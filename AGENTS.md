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
  `lv2_host.rs` is the LV2 plugin and its editor on that same C ABI, with
  the parameters as control ports from port 4; it also writes the bundle's
  two `.ttl` files from the parameter table (`sp9ceverb-lv2-ttl`). The LV2
  name in `lv2_host::URI` is fixed for good.
  `shell_host.rs` is the C ABI the SDK's VST3, AU and AAX shells link:
  `op_shell_*` (audio and tail, the SDK's `export_shell!`) and `op_editor_*`
  (the same editor, gestures sent to the host on the main thread).

## Constraints

- Realtime-safe engine: no allocation, no locks, no panics on the process path
  (`crates/engine/tests/no_alloc.rs`).
- Adding a voicing must leave the others bit-identical unless that is the point
  of the change; the Chaos path is skipped entirely outside its own voicing.
- Chaos output stays at or below -0.5 dBFS and is deterministic.
- Every build is universal unless `SP9CE_ARCHS` names one architecture
  (`scripts/sdk_env.sh`); a release is never built that way.
- No machine paths in the repository or the binaries: dependencies are located
  by `scripts/sdk_env.sh`, and `scripts/build.sh` remaps build paths.
- Do not copy code, tables or presets from other reverbs. Read
  `docs/PROVENANCE.md` before changing the engine.
- The reported tail (`plugin::tail_seconds`, one function for every format)
  must cover what the engine really rings: the low-band decay, the plate and
  chamber echoes, the reversed Room pattern, and infinity for Infinite and for Chaos above zero. Change
  it together with anything that lengthens the ring-out.
- Shell trace, for a host that goes silent: create `sp9ceverb-trace.on` in the
  temporary folder before the plugin is first created, and the VST3, AU and
  AAX append what the host is doing (calls, input and output peaks, engine
  values) to `sp9ceverb-trace.log` there while the editor is open (the
  SDK's `shell::trace`, written from `op_editor_set_plain`). It is off otherwise.
- Never rebuild, reinstall or re-sign while a host suite or validator is
  running: it tests the installed bundles. Use `--check-only` for the editor
  checks.
- Who does what: the agent that writes the plugin code does not run plugin
  QA, and the agent that runs the tests and QA does not edit plugin code,
  the SDK shells or the build scripts.
- No commits without the user's approval.
- This is a public repository. Commits are authored by the `MistaMin` GitHub
  account (its noreply address, set in this repository's git config) and
  carry no co-author or "generated with" lines. No names of AI tools, and no
  names of other makers' reverbs, in source, documents or commit messages.
  Planning and session notes stay out of the repository.

## Build and test

```sh
cargo test --workspace --release
scripts/build.sh [--install] [--app]            # CLAP and LV2; --app also builds the standalone
scripts/build_shells.sh [--install] [--check]   # VST3, AU, AAX; --check clicks each editor's preset arrow
                                                # and changes the VST3's rate 1400 times without a host
scripts/build_shells.sh --check-only            # the same checks on what is already built; builds nothing
auval -v aufx Sp9v OpGd
scripts/sign_aax.sh [--install]         # settings in scripts/signing.local.env (untracked)
cd plugin && . ../scripts/sdk_env.sh && cargo_sdk test --release
scripts/cargo_sdk <cargo args>          # the same patches, for tools that run cargo (the test runner, its hooks)
clap-validator validate dist/Sp9ceVerb.clap
```

The plugin builds against OpenPluginSDK 1.0.0-beta, which carries GoodLookinDri
(0.2.1) in its tree. `shell_host.rs` exports the shells' audio ABI with the
SDK's `export_shell!` and adds its own `op_editor_*`.

Host test: the plugin-daw-test skill, `--fx "CLAP: Sp9ceVerb"`. Voicing controls
only act in their own voicing, so the exceptions file gives each one a
`@ Mode=<index/8>` context (Chaos details also need `@ Chaos=0.9`).

Measurement: `cargo build --release -p sp9ce-engine --example probe`, then
`python3 tools/measure.py <tag> <mode> <label> [key=value ...]`.

## Documents and release

- `README.md` is the page people see on GitHub: what it is, pictures, download
  links, the user guide, then the build notes. Keep its version, links and
  format list in step with the newest release.
- `docs/USER_MANUAL.md` is the manual. Its tables repeat
  `crates/engine/src/params.rs`: change them together.
  `python3 tools/build_user_manual.py` builds
  `output/pdf/Sp9ceVerb-User-Manual.pdf` (reportlab and Pillow), which is
  attached to the GitHub release.
- The pictures in `docs/images/` come from the editor's own code:
  `cargo_sdk build --release --bin sp9ceverb-preview`, then
  `sp9ceverb-preview <out.png> [mode 0..8] [scale] [seconds] [chaos] [view 0..2] [browser 0|1] [preset index]`.
  Render them again when the editor changes.
- `CHANGELOG.md`, `RELEASE_NOTES.md` (the text of the GitHub release) and
  `THIRD_PARTY_LICENSES.md` (every linked component; update it with
  `cargo_sdk tree -e normal` when dependencies change).
- A release follows the Make-Plugin-Release procedure: universal macOS
  binaries (arm64 and x86_64) of CLAP, VST3, Audio Unit, LV2 and AAX. The
  owner decided against shipping the standalone. 0.1.0 was arm64 only and had
  no LV2.
- Versions: `plugin/Cargo.toml` holds the full version (`1.1.0-beta`), which
  hosts show. Bundle `Info.plist` files get the numbers alone (`1.1.0`).
  Documents, the tag and the file names use the owner's wording (`1.1-Beta`).
