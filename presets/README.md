# Presets

One folder per voicing, one text file per preset.

```text
# A comment.
name = Recital Hall
mode = hall
ReverbTime = 1.4
```

- `mode` is the voicing key (`room`, `hall`, `plate`, `chamber`,
  `concert_hall`, `random_hall`, `vintage_plate`, `ambience`, `chaos`).
- Every other key is a parameter identifier from `crates/engine/src/params.rs`
  with a plain value in that parameter's unit (seconds, hertz).
- A parameter the file leaves out keeps the default of its voicing. A file may
  only set parameters its voicing exposes.
- `name` matches the file name, and names are unique across folders.
- `Mix` values assume an insert. On a send, set Mix to 1.

`sp9ce_engine::preset::parse` reads the format, and
`crates/engine/tests/presets.rs` loads and renders every file here. The plugin
embeds this folder when it is built (`plugin/build.rs`) and lists it in the
editor's preset browser, one column per space. Add a file and rebuild.

## Where the values come from

The presets were written for this project from general room acoustics and
common mixing practice. None is taken from another reverb's preset list
(`docs/PROVENANCE.md`). Each file's first line says what it is based on. The
figures used:

- Reverberation time by kind of room: treated studio rooms about 0.3 to 0.6 s,
  classrooms 0.4 to 0.6 s, lecture halls 1.0 to 1.5 s, chamber-music halls
  about 1.4 s, concert halls 1.4 to 2.4 s, churches from 2 s to 10 s. An
  audience shortens the decay.
- Concert-hall warmth: low-frequency decay 1.1 to 1.3 times the mid decay, and
  a first reflection within about 20 ms in an intimate hall.
- Plates: the damper sets the decay, up to about five seconds in the mids when
  backed off; old steel plates lose the top end in under a second.
- Pre-delay in a mix: about 20 to 40 ms on a voice, 5 to 25 ms on a snare, with
  snare decays under about a second.
