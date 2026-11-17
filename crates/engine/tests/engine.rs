use sp9ce_engine::{params, Engine, Mode, ParamId};

const FS: f32 = 48_000.0;

fn noise(n: usize, seed: u32) -> Vec<f32> {
    let mut s = seed | 1;
    (0..n)
        .map(|_| {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            (s as f32 / u32::MAX as f32) * 2.0 - 1.0
        })
        .collect()
}

fn render(e: &mut Engine, il: &[f32], ir: &[f32], block: usize) -> (Vec<f32>, Vec<f32>) {
    let mut ol = vec![0.0; il.len()];
    let mut or = vec![0.0; il.len()];
    let mut pos = 0;
    while pos < il.len() {
        let n = block.min(il.len() - pos);
        let (a, b) = (&mut ol[pos..pos + n], &mut or[pos..pos + n]);
        e.process(&il[pos..pos + n], &ir[pos..pos + n], a, b);
        pos += n;
    }
    (ol, or)
}

fn impulse(n: usize) -> Vec<f32> {
    let mut v = vec![0.0; n];
    v[0] = 1.0;
    v
}

fn rms_db(x: &[f32]) -> f32 {
    let e: f32 = x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32;
    10.0 * e.max(1e-30).log10()
}

fn wet(mode: Mode) -> Engine {
    let mut e = Engine::new(mode);
    e.set_param(ParamId::Mix, 1.0);
    e
}

#[test]
fn specs_are_sane_and_cover_every_parameter() {
    let mut seen = [false; ParamId::COUNT];
    for m in Mode::ALL {
        for s in params::specs(m) {
            assert!(s.min < s.max, "{:?} {:?}", m, s.id);
            assert!(s.min <= s.default && s.default <= s.max, "{:?} {:?}", m, s.id);
            seen[s.id.index()] = true;
        }
    }
    assert!(seen.iter().all(|b| *b), "a parameter is exposed by no voicing");
    for (i, id) in ParamId::ALL.iter().enumerate() {
        assert_eq!(id.index(), i);
    }
}

#[test]
fn set_param_clamps_and_rejects_non_finite() {
    let mut e = Engine::new(Mode::Hall);
    e.set_param(ParamId::ReverbTime, 1.0e9);
    assert_eq!(e.param(ParamId::ReverbTime), 20.0);
    e.set_param(ParamId::Mix, f32::NAN);
    assert_eq!(e.param(ParamId::Mix), 0.0);
}

#[test]
fn silence_in_gives_silence_out() {
    for m in Mode::ALL {
        let mut e = wet(m);
        let z = vec![0.0; 9_600];
        let (l, r) = render(&mut e, &z, &z, 256);
        assert!(l.iter().chain(r.iter()).all(|v| v.abs() < 1.0e-12), "{m:?}");
    }
}

#[test]
fn dry_path_has_zero_latency() {
    let mut e = Engine::new(Mode::Plate);
    e.set_param(ParamId::Mix, 0.0);
    let x = noise(4_800, 7);
    let (l, r) = render(&mut e, &x, &x, 64);
    assert_eq!(e.latency(), 0);
    for i in 0..x.len() {
        assert!((l[i] - x[i]).abs() < 1.0e-6 && (r[i] - x[i]).abs() < 1.0e-6);
    }
}

#[test]
fn output_is_independent_of_block_size() {
    for m in Mode::ALL {
        let x = noise(24_000, 3);
        let y = noise(24_000, 11);
        let reference = render(&mut wet(m), &x, &y, 32);
        for block in [1usize, 33, 500, 4096] {
            let other = render(&mut wet(m), &x, &y, block);
            assert!(reference == other, "{m:?} differs at block size {block}");
        }
    }
}

#[test]
fn every_voicing_responds_and_stays_finite() {
    for m in Mode::ALL {
        let mut e = wet(m);
        let x = impulse(96_000);
        let (l, r) = render(&mut e, &x, &x, 128);
        assert!(l.iter().chain(r.iter()).all(|v| v.is_finite()), "{m:?}");
        assert!(rms_db(&l[..24_000]) > -70.0, "{m:?} is silent");
        assert!(rms_db(&l[72_000..]) < rms_db(&l[..24_000]) - 20.0, "{m:?} does not decay");
    }
}

#[test]
fn reverb_time_sets_the_decay_rate() {
    for rt in [1.0f32, 2.0, 4.0] {
        let mut e = wet(Mode::Hall);
        e.set_param(ParamId::ReverbTime, rt);
        e.set_param(ParamId::HfDamp, 20_000.0);
        e.set_param(ParamId::Spin, 0.0);
        e.set_param(ParamId::Wander, 0.0);
        let x = impulse((FS * rt * 1.2) as usize);
        let (l, _) = render(&mut e, &x, &x, 256);
        let w = (FS * 0.1 * rt) as usize;
        let a = (FS * 0.4 * rt) as usize;
        let b = (FS * 0.9 * rt) as usize;
        let slope = (rms_db(&l[b..b + w]) - rms_db(&l[a..a + w])) / (0.5 * rt);
        let measured = -60.0 / slope;
        assert!((measured / rt - 1.0).abs() < 0.12, "rt {rt}: measured {measured}");
    }
}

#[test]
fn bass_multiplier_lengthens_only_the_low_band() {
    let tail_level = |bass: f32, hz: f32| {
        let mut e = wet(Mode::Chamber);
        e.set_param(ParamId::ReverbTime, 1.0);
        e.set_param(ParamId::BassRt, bass);
        e.set_param(ParamId::BassXover, 500.0);
        e.set_param(ParamId::EchoLTime, 0.0);
        e.set_param(ParamId::EchoRTime, 0.0);
        let n = 96_000;
        let x: Vec<f32> = (0..n)
            .map(|i| {
                if i < 12_000 {
                    // Hann-windowed burst: no broadband click at the edges.
                    let w = 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / 12_000.0).cos();
                    w * (std::f32::consts::TAU * hz * i as f32 / FS).sin()
                } else {
                    0.0
                }
            })
            .collect();
        let (l, _) = render(&mut e, &x, &x, 256);
        rms_db(&l[60_000..72_000]) - rms_db(&l[3_000..9_000])
    };
    let (lo3, lo1, hi3, hi1) =
        (tail_level(3.0, 80.0), tail_level(1.0, 80.0), tail_level(3.0, 4_000.0), tail_level(1.0, 4_000.0));
    assert!(lo3 > lo1 + 15.0);
    assert!((hi3 - hi1).abs() < 3.0, "high band moved: {hi3} vs {hi1}");
}

#[test]
fn width_zero_is_mono_and_level_is_linear() {
    let x = impulse(24_000);
    let mut e = wet(Mode::RandomHall);
    e.set_param(ParamId::Width, 0.0);
    let (l, r) = render(&mut e, &x, &x, 256);
    assert!(l.iter().zip(r.iter()).all(|(a, b)| (a - b).abs() < 1.0e-7));

    let mut a = wet(Mode::Plate);
    let mut b = wet(Mode::Plate);
    b.set_param(ParamId::Level, 0.5);
    let (la, _) = render(&mut a, &x, &x, 256);
    let (lb, _) = render(&mut b, &x, &x, 256);
    assert!(la.iter().zip(lb.iter()).all(|(p, q)| (0.5 * p - q).abs() < 1.0e-6));
}

#[test]
fn infinite_holds_the_tail_and_blocks_new_input() {
    let mut e = wet(Mode::Plate);
    let x = noise(48_000, 5);
    let _ = render(&mut e, &x, &x, 256);
    e.set_param(ParamId::Infinite, 1.0);
    let more = noise(48_000 * 6, 9);
    let (l, _) = render(&mut e, &more, &more, 256);
    let early = rms_db(&l[48_000..96_000]);
    let late = rms_db(&l[48_000 * 5..]);
    assert!((early - late).abs() < 1.5, "hold drifts: {early} -> {late}");
}

#[test]
fn extreme_settings_stay_bounded() {
    for m in Mode::ALL {
        let mut e = wet(m);
        for s in params::specs(m) {
            if s.id != ParamId::Mode && s.id != ParamId::Infinite {
                e.set_param(s.id, s.max);
            }
        }
        let x = noise(48_000 * 8, 21);
        let (l, r) = render(&mut e, &x, &x, 512);
        let peak = l.iter().chain(r.iter()).fold(0.0f32, |a, v| a.max(v.abs()));
        assert!(peak.is_finite() && peak < 200.0, "{m:?} peak {peak}");
    }
}

#[test]
fn mode_can_be_switched_while_running() {
    let mut e = Engine::new(Mode::Room);
    let x = noise(4_800, 2);
    for m in Mode::ALL {
        e.set_param(ParamId::Mode, m.index() as f32);
        let (l, _) = render(&mut e, &x, &x, 128);
        assert_eq!(e.mode(), m);
        assert!(l.iter().all(|v| v.is_finite()));
    }
}

#[test]
fn chaos_at_zero_is_the_plain_reverb_and_other_modes_never_run_it() {
    let x = noise(48_000, 4);
    let mut a = wet(Mode::Chaos);
    let mut b = wet(Mode::RandomHall);
    assert!(render(&mut a, &x, &x, 256) == render(&mut b, &x, &x, 256));
    // The chaos controls do nothing outside the chaos voicing.
    let mut c = wet(Mode::Hall);
    let mut d = wet(Mode::Hall);
    d.set_param(ParamId::Chaos, 1.0);
    assert!(render(&mut c, &x, &x, 256) == render(&mut d, &x, &x, 256));
}

#[test]
fn chaos_sustains_but_stays_under_the_ceiling() {
    for kind in 0..4 {
        let mut e = wet(Mode::Chaos);
        e.set_param(ParamId::Chaos, 1.0);
        e.set_param(ParamId::ChaosNonlin, kind as f32);
        let mut x = noise(48_000 * 12, 31);
        for v in x[48_000..].iter_mut() {
            *v = 0.0;
        }
        let (l, r) = render(&mut e, &x, &x, 512);
        let peak = l.iter().chain(r.iter()).fold(0.0f32, |a, v| a.max(v.abs()));
        assert!(peak.is_finite() && peak <= 0.945, "nonlinearity {kind}: peak {peak}");
        let late = rms_db(&l[48_000 * 10..]);
        assert!(late > -60.0, "nonlinearity {kind}: the tail died ({late} dB)");
        let dc = l.iter().sum::<f32>() / l.len() as f32;
        assert!(dc.abs() < 1.0e-3, "nonlinearity {kind}: dc {dc}");
    }
}

#[test]
fn chaos_is_deterministic() {
    let x = noise(96_000, 8);
    let run = || {
        let mut e = wet(Mode::Chaos);
        e.set_param(ParamId::Chaos, 0.9);
        render(&mut e, &x, &x, 256)
    };
    assert!(run() == run());
}

#[test]
fn ambience_is_dense_at_once_and_short() {
    let mut e = wet(Mode::Ambience);
    e.set_param(ParamId::Predelay, 0.0);
    e.set_param(ParamId::ReverbTime, 0.4);
    let x = impulse(48_000);
    let (l, _) = render(&mut e, &x, &x, 256);
    // Echo density in the first 5..25 ms: share of samples beyond one standard deviation,
    // normalised so that Gaussian noise scores 1.
    let seg = &l[240..1200];
    let mean = seg.iter().sum::<f32>() / seg.len() as f32;
    let sd = (seg.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / seg.len() as f32).sqrt();
    let density = seg.iter().filter(|v| (**v - mean).abs() > sd).count() as f32 / seg.len() as f32 / 0.3173;
    assert!(density > 0.75, "front is sparse: {density}");
    assert!(rms_db(&l[36_000..]) < rms_db(&l[..4_800]) - 40.0, "ambience rings on");
}

#[test]
fn ambience_at_five_times_its_size_is_longer_but_still_dense_and_no_louder() {
    let render_at = |size: f32| {
        let mut e = wet(Mode::Ambience);
        e.set_param(ParamId::AmbSize, size);
        let x = impulse(FS as usize * 2);
        let (l, r) = render(&mut e, &x, &x, 256);
        l.iter().zip(&r).map(|(a, b)| 0.5 * (a + b)).collect::<Vec<f32>>()
    };
    let (small, large) = (render_at(1.0), render_at(params::AMB_SIZE_MAX));
    assert!(large.iter().all(|v| v.is_finite()));
    // Longer: more of its energy arrives after 300 ms.
    let late = |x: &[f32]| rms_db(&x[14_400..48_000]) - rms_db(&x[..14_400]);
    assert!(late(&large) > late(&small) + 6.0, "large {} small {}", late(&large), late(&small));
    // Still dense: between 20 and 120 ms no more than a third of the samples are silent gaps.
    let window = &large[960..5_760];
    let floor = 10f32.powf((rms_db(window) - 30.0) / 20.0);
    let gaps = window.iter().filter(|v| v.abs() < floor).count() as f32 / window.len() as f32;
    assert!(gaps < 0.33, "sparse: {gaps}");
    // No louder overall.
    assert!((rms_db(&large) - rms_db(&small)).abs() < 1.5);
}

#[test]
fn the_longest_reversed_pattern_fits_the_input_line_at_every_rate() {
    // 0.4 s of predelay and 0.5 s x 4: the last and loudest tap is 2.4 s after the input.
    for fs in [32_000.0, 48_000.0, 64_000.0] {
        let mut e = wet(Mode::Room);
        e.activate(fs);
        e.set_param(ParamId::Reverse, 1.0);
        e.set_param(ParamId::RoomScale, 4.0);
        e.set_param(ParamId::ReverbTime, 0.1);
        e.set_param(ParamId::Predelay, 0.2);
        e.set_param(ParamId::EarlyPredelay, 0.2);
        let x = impulse((fs * 3.0) as usize);
        let (l, _) = render(&mut e, &x, &x, 512);
        let peak = (0..l.len()).max_by(|a, b| l[*a].abs().total_cmp(&l[*b].abs())).unwrap();
        assert!((peak as f32 / fs - 2.4).abs() < 0.01, "{fs} Hz: loudest tap at {} s", peak as f32 / fs);
    }
}
