//! Black-box measurement probe: renders one voicing of the unified engine with arbitrary parameter
//! values and a chosen excitation to a 32-bit float stereo WAV.
//!
//! Usage:
//!   probe <product> <out.wav> [key=value ...]
//!
//! Keys:
//!   seconds=<f>     length of the captured render (default 4)
//!   sr=<f>          sample rate (default 48000)
//!   block=<n>       host block size (default 256)
//!   preroll=<f>     seconds of silence processed before capture, so the
//!                   parameter gliders have settled (default 2)
//!   offline=<0|1>   host offline-render flag (default 0)
//!   excite=<name>   impulse | impulse_l | impulse_r | tone | noise | burst
//!   freq=<hz>       tone frequency (default 1000)
//!   amp=<f>         excitation amplitude (default 1.0; tone/noise 0.25)
//!   dur=<f>         tone/noise/burst duration in seconds (default 0.5)
//!   cal_early= cal_tail= cal_side=  override the voicing trims (all three)
//!   <ParamName>=<f> any engine parameter by identifier, e.g. ReverbTime=2.5
//!   <ParamName>@<t>=<f>  change a parameter t seconds into the capture
//!
//! `probe list <product>` prints the parameter table of a product.

use sp9ce_engine::{params, Engine, Mode, ParamId};

fn product(s: &str) -> Mode {
    Mode::from_key(s).unwrap_or_else(|| {
        eprintln!("unknown mode '{s}'");
        std::process::exit(2);
    })
}

fn param(name: &str) -> Option<ParamId> {
    ParamId::ALL
        .iter()
        .copied()
        .find(|id| format!("{id:?}").eq_ignore_ascii_case(name))
}

fn write_wav_f32(path: &str, left: &[f32], right: &[f32], sample_rate: u32) {
    let data_len = (left.len() * 8) as u32;
    let mut b = Vec::with_capacity(44 + data_len as usize);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data_len).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&3u16.to_le_bytes()); // IEEE float
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&sample_rate.to_le_bytes());
    b.extend_from_slice(&(sample_rate * 8).to_le_bytes());
    b.extend_from_slice(&8u16.to_le_bytes());
    b.extend_from_slice(&32u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data_len.to_le_bytes());
    for (l, r) in left.iter().zip(right.iter()) {
        b.extend_from_slice(&l.to_le_bytes());
        b.extend_from_slice(&r.to_le_bytes());
    }
    std::fs::write(path, b).expect("write wav");
}

/// Small deterministic noise source (xorshift), independent of the engine.
struct Rng(u32);
impl Rng {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 3 && args[1] == "list" {
        for s in params::specs(product(&args[2])) {
            println!(
                "{:?}\t{}\tmin={}\tmax={}\tdefault={}\tsteps={}\tunit={}",
                s.id, s.name, s.min, s.max, s.default, s.steps, s.unit
            );
        }
        return;
    }
    if args.len() < 3 {
        eprintln!("usage: probe <product> <out.wav> [key=value ...] | probe list <product>");
        std::process::exit(2);
    }
    let kind = product(&args[1]);
    let out_path = &args[2];

    let mut seconds = 4.0f32;
    let mut sr = 48_000.0f32;
    let mut block = 256usize;
    let mut preroll = 2.0f32;
    let mut offline = false;
    let mut excite = String::from("impulse");
    let mut freq = 1000.0f32;
    let mut amp: Option<f32> = None;
    let mut dur = 0.5f32;
    let mut cal: [Option<f32>; 3] = [None; 3];
    let mut initial: Vec<(ParamId, f32)> = Vec::new();
    let mut timed: Vec<(f32, ParamId, f32)> = Vec::new();

    for a in &args[3..] {
        let Some((k, v)) = a.split_once('=') else {
            eprintln!("bad argument '{a}' (expected key=value)");
            std::process::exit(2);
        };
        let num = || -> f32 {
            v.parse().unwrap_or_else(|_| {
                eprintln!("bad number in '{a}'");
                std::process::exit(2);
            })
        };
        match k {
            "seconds" => seconds = num(),
            "sr" => sr = num(),
            "block" => block = num() as usize,
            "preroll" => preroll = num(),
            "offline" => offline = num() > 0.5,
            "excite" => excite = v.to_string(),
            "freq" => freq = num(),
            "amp" => amp = Some(num()),
            "dur" => dur = num(),
            "cal_early" => cal[0] = Some(num()),
            "cal_tail" => cal[1] = Some(num()),
            "cal_side" => cal[2] = Some(num()),
            _ => {
                let (name, at) = match k.split_once('@') {
                    Some((n, t)) => (n, Some(t.parse::<f32>().expect("time"))),
                    None => (k, None),
                };
                let Some(id) = param(name) else {
                    eprintln!("unknown key or parameter '{name}'");
                    std::process::exit(2);
                };
                if !params::specs(kind).any(|s| s.id == id) {
                    eprintln!("warning: {id:?} is not exposed by this product");
                }
                match at {
                    Some(t) => timed.push((t, id, num())),
                    None => initial.push((id, num())),
                }
            }
        }
    }
    let block = block.clamp(1, 8192);
    let amp = amp.unwrap_or(match excite.as_str() {
        "tone" | "noise" | "burst" => 0.25,
        _ => 1.0,
    });

    let mut engine = Engine::new(kind);
    engine.set_param(ParamId::Mix, 1.0); // wet only unless overridden
    for (id, v) in &initial {
        engine.set_param(*id, *v);
    }
    if let [Some(e), Some(t), Some(sd)] = cal {
        engine.set_calibration(e, t, sd);
    }
    engine.activate(sr);
    let _ = offline; // this engine renders identically online and offline

    let pre = (preroll * sr) as usize;
    let n = (seconds * sr) as usize;
    let total = pre + n;
    let dur_n = (dur * sr) as usize;
    let mut rng = Rng(0x1234_5678);
    timed.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut next_timed = 0usize;

    let mut in_l = vec![0.0f32; block];
    let mut in_r = vec![0.0f32; block];
    let mut out_l = vec![0.0f32; block];
    let mut out_r = vec![0.0f32; block];
    let mut acc_l = vec![0.0f32; n];
    let mut acc_r = vec![0.0f32; n];

    let mut pos = 0usize;
    while pos < total {
        let len = block.min(total - pos);
        while next_timed < timed.len() && pos >= pre + (timed[next_timed].0 * sr) as usize {
            engine.set_param(timed[next_timed].1, timed[next_timed].2);
            next_timed += 1;
        }
        for j in 0..len {
            let (mut l, mut r) = (0.0f32, 0.0f32);
            if pos + j >= pre {
                let t = pos + j - pre;
                match excite.as_str() {
                    "impulse" => {
                        if t == 0 {
                            l = amp;
                            r = amp;
                        }
                    }
                    "impulse_l" => {
                        if t == 0 {
                            l = amp;
                        }
                    }
                    "impulse_r" => {
                        if t == 0 {
                            r = amp;
                        }
                    }
                    "tone" => {
                        if t < dur_n {
                            let x = amp * (std::f32::consts::TAU * freq * t as f32 / sr).sin();
                            l = x;
                            r = x;
                        }
                    }
                    "noise" => {
                        if t < dur_n {
                            l = amp * rng.next();
                            r = amp * rng.next();
                        }
                    }
                    "burst" => {
                        if t < dur_n {
                            let tt = t as f32;
                            let x = amp * (tt * 0.037).sin() * (tt * 0.009).sin();
                            l = x;
                            r = x;
                        }
                    }
                    other => {
                        eprintln!("unknown excitation '{other}'");
                        std::process::exit(2);
                    }
                }
            }
            in_l[j] = l;
            in_r[j] = r;
        }
        engine.process(&in_l[..len], &in_r[..len], &mut out_l[..len], &mut out_r[..len]);
        for j in 0..len {
            if pos + j >= pre {
                acc_l[pos + j - pre] = out_l[j];
                acc_r[pos + j - pre] = out_r[j];
            }
        }
        pos += len;
    }
    write_wav_f32(out_path, &acc_l, &acc_r, sr as u32);
}
