# Sp9ceVerb 1.0.1-Beta

Sp9ceVerb is a reverb plugin with nine spaces on one engine: Room, Hall, Plate, Chamber, Concert Hall, Random Hall, Vintage Plate, Ambience and Chaos. This is a fix release of the beta and is marked as a pre-release.

## New since 1.0-Beta

- The reported tail now includes the reversed Room pattern (up to 2 s at Room Scale 4). Before, a host could cut its ring-out short when exporting.
- The reversed Room pattern sounds the same at every sample rate. At 32 and 64 kHz its late taps used to pile up on one position, about 12 dB too loud.
- The other spaces, the presets and the sound are unchanged, and sessions saved with 1.0-Beta open as before.

## Formats and requirements

- CLAP, VST3, Audio Unit, LV2 and AAX (Developer ID signed and PACE signed).
- A Mac with Apple silicon or an Intel processor, macOS 11.0 or later.
- Hosts show the version as `1.0.1-beta`.

## Install

Open `Sp9ceVerb-1.0.1-Beta-macos.pkg` (or the same file inside the disk image) and choose the formats. Installed to:

- `/Library/Audio/Plug-Ins/CLAP`
- `/Library/Audio/Plug-Ins/VST3`
- `/Library/Audio/Plug-Ins/Components`
- `/Library/Audio/Plug-Ins/LV2`
- `/Library/Application Support/Avid/Audio/Plug-Ins`

The manual and licence texts go to `/Library/Application Support/Sp9ceVerb`. The manual is also attached as `Sp9ceVerb-User-Manual.pdf` and is in the repository as `docs/USER_MANUAL.md`.

## Checks it passed

- Engine and plugin unit tests, including new tests for the reversed Room pattern at 32, 48 and 64 kHz and for the reported tail with Reverse on.
- Every binary holds arm64 and x86_64 code with a macOS 11.0 minimum.
- `auval` validation: succeeded as Apple silicon code and as Intel code.
- clap-validator: 34 passed, 0 failed, 10 skipped (features the plugin does not have).
- LV2 description files: 0 errors in `lv2_validate`; all 54 control ports match the parameter table.
- Avid AAX Validator: 6 of 6 tests.
- Scripted host test suite in a DAW, as Apple silicon code: all 9 checks for CLAP, VST3, LV2 and Audio Unit. CLAP also passed all 9 as Intel code, translated by Rosetta.
- Editor checks for VST3 and Audio Unit.
- Pro Tools 2025.10 with the PACE-signed AAX (tested on this code before the version number and a CLAP-only fix were added): it loads under Reverb and OpenGrid in stereo and multi-mono, the editor opens, the controls, voicings and preset arrows work, bypass is clean, bounces are finite with a full tail, automation is written, the session saves and reopens with the same values, and the reported delay is 0.
- The installer is signed with Developer ID Installer, notarized by Apple and stapled.

## Known limits

- The Intel code was tested under Rosetta, not on an Intel Mac. The Intel AAX has not been run at all.
- No Windows or Linux build (the editor is macOS only), and no standalone application.
- AAX and LV2 report no tail length to the host.
- In Pro Tools, a track fed live from a bus with input monitoring kept the same peak reading with Sp9ceVerb on or bypassed, while the plugin's own meter moved. A track playing a clip behaves as expected. The cause is not yet known.
- In a test host with no audio device, a parameter the host changed while nothing played reached the CLAP editor only once audio ran. The VST3 editor followed at once.

## Files

- `Sp9ceVerb-1.0.1-Beta-macos.pkg`: the installer.
- `Sp9ceVerb-1.0.1-Beta-macos.dmg`: the same installer with the manual and licences, as a notarized disk image.
- `Sp9ceVerb-User-Manual.pdf`: the user manual.
- `opengrid-manifest.json`: the manifest the OpenGrid Plugin Manager reads.
- `SHA256SUMS`: SHA-256 of the installer and the disk image.

Source: MIT. Official binaries: freeware (`BINARY_LICENSE.txt`); third-party terms are in `THIRD_PARTY_LICENSES.md`. VST is a trademark of Steinberg Media Technologies GmbH. Audio Units is a trademark of Apple Inc. AAX and Pro Tools are trademarks of Avid Technology, Inc.
