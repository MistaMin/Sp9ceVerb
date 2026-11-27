//! Random parameter, voicing and reset traffic. The output must stay finite,
//! and the dry signal must still pass at Mix 0 afterwards: one bad value kept
//! in the engine would silence everything behind the plugin's output guard.

use sp9ce_engine::{params, Engine, Mode, ParamId};

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }
}

fn run(seed: u64, rounds: usize) -> Result<(), String> {
    let mut rng = Rng(seed | 1);
    let rates = [44_100.0f32, 48_000.0, 96_000.0, 192_000.0, 8_000.0, 1_000.0, 768_000.0];
    let mut e = Engine::new(Mode::Hall);
    let mut fs = 48_000.0f32;
    let mut input = vec![0.0f32; 4096];
    let (mut l, mut r) = (vec![0.0f32; 4096], vec![0.0f32; 4096]);
    for round in 0..rounds {
        match (rng.next() * 12.0) as usize {
            0 => {
                fs = rates[(rng.next() * rates.len() as f32) as usize % rates.len()];
                e.activate(fs);
            }
            1 => e.reset(),
            2 | 3 => e.set_param(ParamId::Mode, (rng.next() * 9.0).floor()),
            _ => {
                let id = ParamId::ALL[(rng.next() * ParamId::COUNT as f32) as usize % ParamId::COUNT];
                let (lo, hi) = params::range(id);
                // Ends and beyond the ends as often as the middle.
                let v = match (rng.next() * 6.0) as usize {
                    0 => lo,
                    1 => hi,
                    2 => lo - 1.0,
                    3 => hi * 2.0 + 1.0,
                    _ => lo + (hi - lo) * rng.next(),
                };
                e.set_param(id, v);
            }
        }
        let n = 1 + (rng.next() * 4095.0) as usize;
        let loud = rng.next() < 0.1;
        for x in input[..n].iter_mut() {
            *x = (rng.next() * 2.0 - 1.0) * if loud { 8.0 } else { 0.5 };
        }
        e.process(&input[..n], &input[..n], &mut l[..n], &mut r[..n]);
        if let Some(i) = (0..n).find(|&i| !l[i].is_finite() || !r[i].is_finite()) {
            return Err(format!("seed {seed} round {round}: non-finite output at sample {i} in {:?}", e.mode()));
        }
    }
    // Whatever happened, the dry path still works.
    e.set_param(ParamId::Mix, 0.0);
    e.set_param(ParamId::Level, 1.0);
    // The same settling time at every rate: 8 blocks at 48 kHz.
    for _ in 0..(8.0 * fs / 48_000.0).ceil().max(8.0) as usize {
        input.fill(0.25);
        e.process(&input, &input, &mut l, &mut r);
    }
    if (l[4095] - 0.25).abs() > 1e-3 || (r[4095] - 0.25).abs() > 1e-3 {
        return Err(format!("seed {seed}: dry path gives {} / {} for 0.25", l[4095], r[4095]));
    }
    Ok(())
}

#[test]
fn random_traffic_never_leaves_the_engine_non_finite_or_silent() {
    for seed in 1..=60u64 {
        if let Err(e) = run(seed * 0x9E37_79B9, 400) {
            panic!("{e}");
        }
    }
}

#[test]
fn a_non_finite_input_sample_does_not_stick() {
    for mode in Mode::ALL {
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut e = Engine::new(mode);
            e.set_param(ParamId::Mix, 0.5);
            if mode == Mode::Chaos {
                e.set_param(ParamId::Chaos, 0.8);
            }
            let mut input = vec![0.1f32; 4800];
            let (mut l, mut r) = (vec![0.0f32; 4800], vec![0.0f32; 4800]);
            e.process(&input, &input, &mut l, &mut r);
            input[100] = bad;
            e.process(&input, &input, &mut l, &mut r);
            input[100] = 0.1;
            // Two seconds later the engine is clean again.
            for _ in 0..20 {
                e.process(&input, &input, &mut l, &mut r);
            }
            assert!(l.iter().chain(r.iter()).all(|v| v.is_finite()), "{mode:?}: {bad} stuck in the engine");
        }
    }
}
