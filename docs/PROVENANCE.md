# Provenance

This records what Sp9ceVerb was built from, so nobody has to guess later.

## What the code is

- Every source file in this repository was written for this project.
- The engine's topology (input line, early tap pattern, four to six input
  diffusers, an eight-line feedback network with per-line bass and
  high-frequency decay shelves, mid/side tail, and for Ambience a four-line
  second network) is an ordinary textbook reverb structure, chosen here.
- Delay lengths, diffuser lengths, early tap patterns and modulation rates
  were chosen for this project. The tap patterns are generated procedurally
  from fixed seeds.
- No source code, data table or preset from any other reverb is included.

## What informed it

Sp9ceVerb was built from black-box measurements of an existing reverb.

- The reverb was measured strictly as a black box: impulses, tones and noise
  in, audio out, with every parameter swept (about 3,500 logged renders).
- Nobody working on this engine read that reverb's signal-processing source
  while designing it, and none of its code or data tables is used here.

From those measurements this project took:

- per-voicing level targets at default settings: wet energy of a unit impulse,
  early-to-late balance (C80) and side-to-mid ratio. `tools/calibrate.py`
  holds these numbers and fits three gains per voicing to them;
- control laws that were measured cleanly: linear dry/wet crossfade, linear
  output level, side gain proportional to Width;
- the parameter set and its ranges, as a product brief.

It deliberately did not take the measured delay times, and it does not
reproduce the measured reverb's quirks: here Reverb Time is the 60 dB decay
time, Bass RT lengthens only the low band, and HF Damping acts inside the
decay loop.

Ambience and Chaos were designed from written specifications, not from
measurements of anything.

## What this is not

This note describes how the work was done. It is not a legal opinion about
the measured reverb or about this project; ask a lawyer for that.
