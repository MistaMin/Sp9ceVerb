# Sp9ceVerb 0.1.0

Sp9ceVerb is a reverb plugin with nine spaces on one engine: Room, Hall, Plate, Chamber, Concert Hall, Random Hall, Vintage Plate, Ambience and Chaos. This is the first release and is marked as a pre-release.

## Formats and requirements

- CLAP, VST3, Audio Unit and AAX (Developer ID signed and PACE signed).
- Mac with Apple silicon (arm64), macOS 11.0 or later. No Intel build.

## Install

Open `Sp9ceVerb-0.1.0-macos.pkg` (or the same file inside the disk image) and choose the formats. Installed to:

- `/Library/Audio/Plug-Ins/CLAP`
- `/Library/Audio/Plug-Ins/VST3`
- `/Library/Audio/Plug-Ins/Components`
- `/Library/Application Support/Avid/Audio/Plug-Ins`

The manual and licence texts go to `/Library/Application Support/Sp9ceVerb`. The manual is also attached as `Sp9ceVerb-User-Manual.pdf` and is in the repository as `docs/USER_MANUAL.md`.

## Highlights

- Nine spaces on one engine, with the controls of each space.
- Editor with three displays (Surface, Ocean, Orbit), an output meter, a preset bar and a preset browser; 38 factory presets.
- Tail length reported to the host (CLAP, VST3, Audio Unit); Infinite hold on Plate, Chamber and Vintage Plate; Chaos output stays at or below -0.5 dBFS.

## Checks it passed

- Engine and plugin unit tests.
- `auval` validation: succeeded, 0 warnings.
- Avid AAX Validator: 6 of 6 tests.
- Scripted host test suite in a DAW: all 9 checks for CLAP, VST3 and Audio Unit (7 runs).
- Editor checks for VST3 and Audio Unit.
- The installer is signed with Developer ID Installer, notarized by Apple and stapled. The binaries are arm64 with a macOS 11.0 minimum.

## Known limits

- Apple silicon and macOS 11 or later only.
- No Windows or Linux build (the editor is macOS only).
- No standalone application in this release, and no LV2.
- The AAX has not yet been tested in Pro Tools itself. It passes Avid's AAX Validator and is PACE signed.
- During development the Audio Unit intermittently rendered silence in the host test suite (4 runs). This did not recur in the last 15 Audio Unit runs, and the cause was not identified.
- AAX reports no tail length to the host.

## Files

- `Sp9ceVerb-0.1.0-macos.pkg`: the installer.
- `Sp9ceVerb-0.1.0-macos.dmg`: the same installer with the manual and licences, as a notarized disk image.
- `Sp9ceVerb-User-Manual.pdf`: the user manual.
- `opengrid-manifest.json`: the manifest the OpenGrid Plugin Manager reads.
- `SHA256SUMS`: SHA-256 of the installer and the disk image.

Source: MIT. Official binaries: freeware (`BINARY_LICENSE.txt`); third-party terms are in `THIRD_PARTY_LICENSES.md`. VST is a trademark of Steinberg Media Technologies GmbH. Audio Units is a trademark of Apple Inc. AAX and Pro Tools are trademarks of Avid Technology, Inc.
