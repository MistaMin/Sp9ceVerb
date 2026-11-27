# Sp9ceVerb 1.1-Beta

Sp9ceVerb is a reverb plugin with nine spaces on one engine: Room, Hall, Plate, Chamber, Concert Hall, Random Hall, Vintage Plate, Ambience and Chaos. This release moves the plugin to a new foundation and keeps its sound. It is marked as a pre-release.

## New since 1.0.1-Beta

- Built on OpenPluginSDK 1.0.0-beta. The sound, the presets, the parameters and the plugin and parameter ids are unchanged, and sessions saved with 1.0.1-Beta open with every value intact.
- The plugin runs at the host's real sample rate up to 768 kHz: from 1 kHz as CLAP, LV2 and Audio Unit, from 8 kHz as VST3 and AAX. Above 384 kHz it used to run as if at 384 kHz, with every time and frequency off.
- The VST3 has a bypass parameter, which hosts use for their own bypass switch.
- VST3 and Audio Unit hosts show values as text (`Concert Hall`, `7.46 s`) instead of bare numbers.

## Formats and requirements

- CLAP, VST3, Audio Unit, LV2 and AAX (Developer ID signed and PACE signed).
- A Mac with Apple silicon or an Intel processor, macOS 11.0 or later.
- Hosts show the version as `1.1.0-beta`.

## Install

Open `Sp9ceVerb-1.1-Beta-macos.pkg` (or the same file inside the disk image) and choose the formats. Installed to:

- `/Library/Audio/Plug-Ins/CLAP`
- `/Library/Audio/Plug-Ins/VST3`
- `/Library/Audio/Plug-Ins/Components`
- `/Library/Audio/Plug-Ins/LV2`
- `/Library/Application Support/Avid/Audio/Plug-Ins`

The manual and licence texts go to `/Library/Application Support/Sp9ceVerb`. The manual is also attached as `Sp9ceVerb-User-Manual.pdf` and is in the repository as `docs/USER_MANUAL.md`.

## Checks it passed

- Engine and plugin unit tests, including: sessions saved by 1.0.1-Beta open with every value; the parameter order is the released one; the reverb's timing is right at eleven rates from 1 to 768 kHz.
- The SDK's conformance kit at 22.05 to 384 kHz, over 200 random seeds, and with strict options (no NaN during hostile input, no denormal output).
- Every binary holds arm64 and x86_64 code with a macOS 11.0 minimum.
- `auval` validation succeeded.
- clap-validator: 34 passed, 0 failed, 10 skipped (features the plugin does not have).
- LV2 description files: 0 errors in `lv2_validate`.
- Avid AAX Validator: 6 of 6 tests.
- Scripted host test suite in a DAW: all 9 checks for CLAP, VST3 and LV2. Renders at 44.1 to 768 kHz have the same reverb time within 0.1 %, and the output is sample-identical to 1.0.1-Beta.
- The installer is signed with Developer ID Installer, notarized by Apple and stapled.

## Known limits

- 1.1-Beta has not been tested in Pro Tools. 1.0.1-Beta was, in Pro Tools 2025.10.
- The Intel code has not been run for this release; 1.0.1-Beta's was tested under Rosetta, not on an Intel Mac.
- No Windows or Linux build (the editor is macOS only), and no standalone application.
- AAX and LV2 report no tail length to the host.
- Below 22.05 kHz the reverb measures longer than at the usual rates (about 16 % at 8 kHz). The likely cause is that less of the high end fits under those rates; this is not confirmed.
- Open from 1.0.1-Beta and not tested again: in Pro Tools, a track fed live from a bus with input monitoring kept the same peak reading with Sp9ceVerb on or bypassed; in a test host with no audio device, a parameter the host changed while nothing played reached the CLAP editor only once audio ran.

## Files

- `Sp9ceVerb-1.1-Beta-macos.pkg`: the installer.
- `Sp9ceVerb-1.1-Beta-macos.dmg`: the same installer with the manual and licences, as a notarized disk image.
- `Sp9ceVerb-User-Manual.pdf`: the user manual.
- `opengrid-manifest.json`: the manifest the OpenGrid Plugin Manager reads.
- `SHA256SUMS`: SHA-256 of the installer and the disk image.

Source: MIT. Official binaries: freeware (`BINARY_LICENSE.txt`); third-party terms are in `THIRD_PARTY_LICENSES.md`. VST is a trademark of Steinberg Media Technologies GmbH. Audio Units is a trademark of Apple Inc. AAX and Pro Tools are trademarks of Avid Technology, Inc.
