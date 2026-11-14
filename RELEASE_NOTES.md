# Sp9ceVerb 1.0-Beta

Sp9ceVerb is a reverb plugin with nine spaces on one engine: Room, Hall, Plate, Chamber, Concert Hall, Random Hall, Vintage Plate, Ambience and Chaos. This is a beta and is marked as a pre-release.

## New since 0.1.0

- Every format is a universal binary: Apple silicon (arm64) and Intel (x86_64).
- New format: LV2, with the editor.
- The engine, the presets and the sound are unchanged, and sessions saved with 0.1.0 open as before.

## Formats and requirements

- CLAP, VST3, Audio Unit, LV2 and AAX (Developer ID signed and PACE signed).
- A Mac with Apple silicon or an Intel processor, macOS 11.0 or later.
- Hosts show the version as `1.0.0-beta`.

## Install

Open `Sp9ceVerb-1.0-Beta-macos.pkg` (or the same file inside the disk image) and choose the formats. Installed to:

- `/Library/Audio/Plug-Ins/CLAP`
- `/Library/Audio/Plug-Ins/VST3`
- `/Library/Audio/Plug-Ins/Components`
- `/Library/Audio/Plug-Ins/LV2`
- `/Library/Application Support/Avid/Audio/Plug-Ins`

The manual and licence texts go to `/Library/Application Support/Sp9ceVerb`. The manual is also attached as `Sp9ceVerb-User-Manual.pdf` and is in the repository as `docs/USER_MANUAL.md`.

## Highlights

- Nine spaces on one engine, with the controls of each space.
- Editor with three displays (Surface, Ocean, Orbit), an output meter, a preset bar and a preset browser; 38 factory presets.
- Tail length reported to the host (CLAP, VST3, Audio Unit); Infinite hold on Plate, Chamber and Vintage Plate; Chaos output stays at or below -0.5 dBFS.

## Checks it passed

- Engine and plugin unit tests.
- Every binary holds arm64 and x86_64 code with a macOS 11.0 minimum.
- `auval` validation: succeeded as Apple silicon code and as Intel code.
- clap-validator: 34 passed, 0 failed, 10 skipped (features the plugin does not have).
- LV2 description files: 0 errors in `lv2_validate`; all 54 control ports match the parameter table.
- Avid AAX Validator: 6 of 6 tests.
- Scripted host test suite in a DAW, as Apple silicon code: all 9 checks for CLAP, VST3, LV2 and Audio Unit (the Audio Unit three times). The LV2 shows the plugin's own editor there.
- The same host test suite as Intel code, translated by Rosetta on an Apple silicon Mac: all 9 checks for CLAP, VST3, LV2 and Audio Unit, editors included.
- Intel code: the CLAP, VST3 and LV2 were loaded by small test programs, processed audio, and gave the same output as the Apple silicon code.
- Editor checks for VST3 and Audio Unit.
- The installer is signed with Developer ID Installer, notarized by Apple and stapled.

## Known limits

- The Intel code was tested under Rosetta, not on an Intel Mac. The Intel AAX has not been run at all.
- No Windows or Linux build (the editor is macOS only), and no standalone application.
- The AAX has not yet been tested in Pro Tools itself. It passes Avid's AAX Validator and is PACE signed.
- AAX and LV2 report no tail length to the host.
- In 0.1.0's development the Audio Unit intermittently rendered silence in the host test suite. It has not recurred in the last 18 Audio Unit runs, and the cause was not identified.
- One LV2 host test run was cut short from outside the plugin; four full LV2 runs after it passed all checks.

## Files

- `Sp9ceVerb-1.0-Beta-macos.pkg`: the installer.
- `Sp9ceVerb-1.0-Beta-macos.dmg`: the same installer with the manual and licences, as a notarized disk image.
- `Sp9ceVerb-User-Manual.pdf`: the user manual.
- `opengrid-manifest.json`: the manifest the OpenGrid Plugin Manager reads.
- `SHA256SUMS`: SHA-256 of the installer and the disk image.

Source: MIT. Official binaries: freeware (`BINARY_LICENSE.txt`); third-party terms are in `THIRD_PARTY_LICENSES.md`. VST is a trademark of Steinberg Media Technologies GmbH. Audio Units is a trademark of Apple Inc. AAX and Pro Tools are trademarks of Avid Technology, Inc.
