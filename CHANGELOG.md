# Changelog

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
