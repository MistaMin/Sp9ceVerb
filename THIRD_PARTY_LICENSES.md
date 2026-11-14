# Third-party licenses

Sp9ceVerb's own source code is MIT licensed (see `LICENSE`). The official
binaries are proprietary freeware (see `BINARY_LICENSE.txt`). That is not a
relicensing of the components below, which keep their own terms. Each plugin
bundle carries the licence text of the format SDK it links in its
`Contents/Resources` folder, and the installer and disk image carry copies in
`Licenses/`.

| Component | Used for | License |
|---|---|---|
| OpenPluginSDK | plugin framework and the VST3, Audio Unit and AAX shells | MIT or Apache License 2.0 |
| GoodLookinDri | the editor's drawing and window code (Rust, linked statically) | MIT |
| Rust standard library | engine, plugin and editor code | MIT or Apache License 2.0 |
| fontdue, ttf-parser | lettering | MIT or Apache License 2.0 (fontdue also Zlib) |
| rustfft, num-complex, num-integer, num-traits, primal-check, strength_reduce, transpose | the spectrum in the displays | MIT or Apache License 2.0 |
| hashbrown, ahash, allocator-api2, once_cell, cfg-if, libc, log, bitflags, serde, zerocopy, raw-window-handle | support libraries | MIT or Apache License 2.0 (zerocopy also BSD 2-Clause, raw-window-handle also Zlib) |
| CLAP | CLAP format | MIT |
| LV2 | LV2 format: the interface is written from the specification, and no LV2 code is linked | ISC |
| VST3 SDK (Steinberg) | VST3 format | MIT |
| AudioUnitSDK (Apple) | Audio Unit format | Apache License 2.0 |
| AAX SDK (Avid) | AAX format | Avid AAX Developer Agreement / SDK License |

The AAX SDK is an external proprietary dependency under the Avid AAX Developer
Agreement and is not committed to this repository. Official AAX binaries are
digitally signed with PACE Eden for Pro Tools compatibility.

The standalone application, which is built from source but not part of the
release, also links cpal (Apache License 2.0), coreaudio-rs,
coreaudio-sys, core-foundation-sys, dasp_sample and mach2 (MIT or Apache
License 2.0).

VST is a trademark of Steinberg Media Technologies GmbH. Audio Units is a
trademark of Apple Inc. AAX and Pro Tools are trademarks of Avid Technology,
Inc. Other format and host names are used only to describe compatibility.
