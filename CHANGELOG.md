# Changelog

## Unreleased

- Built on OpenPluginSDK 1.0.0-beta, which carries GoodLookinDri in its tree.
  The VST3, Audio Unit and AAX shells speak its shell ABI 10. The sound, the
  parameters and the plugin and parameter ids are unchanged, and sessions
  saved with 1.0.1-Beta open with every value intact.
- The VST3 has a host bypass parameter, which hosts use for their own
  bypass switch.
- VST3 and Audio Unit hosts show values as text (`Concert Hall`, `7.46 s`)
  instead of bare numbers.
- The plugin runs at its real sample rate up to 768 kHz and tells the host
  so: from 1 kHz as CLAP, LV2 and Audio Unit, from 8 kHz as VST3 and AAX.
  Above 384 kHz it used to run as if at 384 kHz, with every time and
  frequency off.
- Hosts show the version as `1.1.0-beta`.

## 1.0.1-Beta - 2026-11-22

- The reported tail now includes the reversed Room pattern (up to 2 s at
  Room Scale 4). Before, a host could cut its ring-out short when exporting.
- The reversed Room pattern sounds the same at every sample rate. At 32 and
  64 kHz its late taps used to pile up on one position, about 12 dB too loud.
- The other spaces, the presets and the sound are unchanged, and sessions
  saved with 1.0-Beta open as before.
- Hosts show the version as `1.0.1-beta`.

## 1.0-Beta - 2026-11-14

- Every format is now a universal binary: Apple silicon (arm64) and Intel
  (x86_64), macOS 11.0 or later.
- New format: LV2, with the editor. The parameters are control ports, which
  the host saves.
- The installer has a fifth choice, LV2.
- The engine, the presets and the sound are unchanged from 0.1.0, and
  sessions saved with 0.1.0 open as before.
- Hosts show the version as `1.0.0-beta`.

Known limits: no Windows or Linux build and no standalone app; the AAX has not
been tested inside Pro Tools itself; AAX and LV2 report no tail length.

## 0.1.0 - 2026-11-14

First release. macOS, Apple silicon (arm64), macOS 11.0 or later.

- Nine spaces on one engine: Room, Hall, Plate, Chamber, Concert Hall, Random
  Hall, Vintage Plate, Ambience and Chaos.
- Editor with three displays (Surface, Ocean, Orbit), an output meter, a preset
  bar and a preset browser. 38 factory presets across the nine spaces.
- Formats: CLAP, VST3, Audio Unit and AAX (Developer ID signed, PACE signed).
- Tail length reported to the host in CLAP, VST3 and Audio Unit; Infinite and
  Chaos report an infinite tail. AAX reports no tail length.
- Signed with Developer ID, notarized and stapled installer with one choice per format.

Known limits: no Windows or Linux build, no standalone app and no LV2 in this
release; the AAX has not been tested inside Pro Tools itself.
