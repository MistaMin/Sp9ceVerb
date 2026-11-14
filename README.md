# Sp9ceVerb

A reverb plugin with nine spaces on one engine, written in Rust.

| Space | Character |
|---|---|
| Room, Hall, Plate, Chamber, Concert Hall, Random Hall, Vintage Plate | the seven classic programs |
| Ambience | short, dense and invisible: a front softener, a small room and a quiet plateau |
| Chaos | deliberately unstable: in-loop nonlinearity, governed self-oscillation, pitch drift and scramble |

The editor shows the reverb as it works, in three views: a 3D decay surface
(frequency across, time into the distance, level up), an ocean of waves driven
by the output, and an orbit view with the source in the middle.

## Layout

- `crates/engine` (`sp9ce-engine`): the DSP. Framework-free, no dependencies,
  no allocation on the audio path. One signal path, nine voicings.
- `plugin` (`sp9ceverb`): the plugin on OpenPluginSDK, the GoodLookinDri editor,
  the CLAP entry with its editor bridge, and the standalone app.
- `scripts/build.sh`: builds and bundles `dist/Sp9ceVerb.clap` and `dist/Sp9ceVerb.app`.
- `scripts/build_shells.sh`: builds `dist/Sp9ceVerb.vst3`, `.component` and `.aaxplugin`
  on OpenPluginSDK's native shells. Needs the VST3 SDK, AudioUnitSDK and the AAX SDK.
- `scripts/sign_aax.sh`: signs the AAX for Pro Tools (Developer ID and PACE) into `dist/signed/`.
- `presets/`: factory presets as text files, one folder per space (`presets/README.md`).
  They are built into the plugin and chosen from the preset bar above the display.
- `tools/`: the measurement runner and the level calibration.
- `docs/PROVENANCE.md`: where the design came from, stated plainly.

## Build and test

```sh
cargo test --workspace --release      # engine tests (no SDK needed)
scripts/build.sh                      # plugin + app into dist/
scripts/build.sh --install            # also copy the CLAP to ~/Library/Audio/Plug-Ins/CLAP
```

OpenPluginSDK and GoodLookinDri are found at build time (a `Dependencies`
folder next to or above this one, or `OPENPLUGIN_SDK` / `GOODLOOKINDRI`).
To run cargo on the plugin crate directly:

```sh
cd plugin && . ../scripts/sdk_env.sh && cargo_sdk test --release
```

## Formats

| Format | State |
|---|---|
| CLAP | built, with the editor. Passes clap-validator and the REAPER suite |
| Standalone (macOS) | built. Plays a demo pattern; `--input` processes the input device, `--silent` opens no audio device |
| VST3, AU (macOS) | built by `scripts/build_shells.sh`, with the editor. The AU passes `auval` |
| AAX (macOS) | built by `scripts/build_shells.sh`, with the editor. It links the Avid SDK, so the binary is not an MIT-only artifact, and Pro Tools only loads it once signed (`scripts/sign_aax.sh`) |

macOS on Apple silicon only: the editor window and its lettering use Cocoa and Core Text.

## License

Source: MIT, copyright (c) 2026 Marcos Quirino Deida (`LICENSE`). Official binaries: proprietary freeware (`BINARY_LICENSE.txt`).
