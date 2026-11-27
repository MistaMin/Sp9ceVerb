//! Sp9ceVerb in its own window. A built-in plucked pattern plays through the
//! reverb to the default output device.
//!
//!   sp9ceverb-app            play the demo pattern
//!   sp9ceverb-app --silent   run the engine without opening an audio device
//!   sp9ceverb-app --input    process the default input device instead (use headphones)

use openplugin::prelude::Plugin;
use std::sync::Arc;
use sp9ceverb::gui::Editor;
use sp9ceverb::plugin::{GuiEvent, OnePlugin, Shared};

struct Voice {
    plugin: OnePlugin,
    shared: Arc<Shared>,
    io: Vec<Vec<f32>>,
    n: u64,
    sr: f32,
}

impl Voice {
    fn new(sr: f32) -> Self {
        let mut plugin = OnePlugin::default();
        let shared = plugin.shared();
        plugin.prepare(sr, 4096, 2);
        plugin.set_param("mix", 0.45);
        Self { plugin, shared, io: vec![vec![0.0; 4096], vec![0.0; 4096]], n: 0, sr }
    }

    /// Render `frames` of the demo pattern (or of `input`) through the reverb.
    fn render(&mut self, frames: usize, input: Option<&[f32]>, channels: usize) {
        while let Some(e) = self.shared.queue.pop() {
            if let GuiEvent::Value(i, v) = e {
                self.plugin.set_param_index(i, v);
            }
        }
        for i in 0..frames {
            let (l, r) = match input {
                Some(buf) if channels >= 2 => (buf[i * channels], buf[i * channels + 1]),
                Some(buf) => (buf[i * channels], buf[i * channels]),
                None => {
                    let t = self.n as f32 / self.sr;
                    let step = (t * 2.0) as usize;
                    let beat = (t * 2.0).fract();
                    let note = [196.0, 293.7, 246.9, 392.0, 329.6, 261.6, 440.0, 220.0][step % 8];
                    let env = (-beat * 11.0).exp() * if step % 4 == 3 { 0.0 } else { 1.0 };
                    let w = t * note * std::f32::consts::TAU;
                    let x = 0.22 * env * (w.sin() + 0.35 * (2.0 * w).sin() + 0.12 * (3.01 * w).sin());
                    (x, x)
                }
            };
            self.io[0][i] = l;
            self.io[1][i] = r;
            self.n += 1;
        }
        let [l, r] = &mut self.io[..] else { return };
        self.plugin.process(&mut [&mut l[..], &mut r[..]], frames);
    }
}

fn main() {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    let args: Vec<String> = std::env::args().collect();
    let silent = args.iter().any(|a| a == "--silent");
    let use_input = args.iter().any(|a| a == "--input");

    let host = cpal::default_host();
    let mut streams = Vec::new();
    let shared;
    if silent {
        let mut voice = Voice::new(48_000.0);
        shared = voice.shared.clone();
        std::thread::spawn(move || loop {
            voice.render(480, None, 2);
            std::thread::sleep(std::time::Duration::from_millis(10));
        });
    } else {
        let device = host.default_output_device().expect("no audio output device");
        let config = device.default_output_config().expect("no output format");
        let sr = config.sample_rate().0 as f32;
        let channels = config.channels() as usize;
        let mut voice = Voice::new(sr);
        shared = voice.shared.clone();
        let (tx, rx) = std::sync::mpsc::sync_channel::<Vec<f32>>(64);
        if use_input {
            if let Some(input) = host.default_input_device() {
                if let Ok(in_cfg) = input.default_input_config() {
                    let in_ch = in_cfg.channels() as usize;
                    let stream = input
                        .build_input_stream(
                            &in_cfg.into(),
                            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                                // Interleaved stereo, whatever the device width.
                                let mut v = Vec::with_capacity(data.len() / in_ch * 2);
                                for f in data.chunks(in_ch) {
                                    v.push(f[0]);
                                    v.push(f[in_ch.min(2) - 1]);
                                }
                                let _ = tx.try_send(v);
                            },
                            |e| eprintln!("input stream: {e}"),
                            None,
                        )
                        .expect("input stream");
                    stream.play().expect("start input");
                    streams.push(stream);
                }
            }
        }
        let mut pending: Vec<f32> = Vec::new();
        let stream = device
            .build_output_stream(
                &config.into(),
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    let frames = (data.len() / channels).min(4096);
                    if use_input {
                        while let Ok(v) = rx.try_recv() {
                            pending.extend_from_slice(&v);
                        }
                        if pending.len() < frames * 2 {
                            pending.resize(frames * 2, 0.0);
                        }
                        let block: Vec<f32> = pending.drain(..frames * 2).collect();
                        voice.render(frames, Some(&block), 2);
                    } else {
                        voice.render(frames, None, 2);
                    }
                    for (i, f) in data.chunks_mut(channels).enumerate().take(frames) {
                        f[0] = voice.io[0][i];
                        if channels > 1 {
                            f[1] = voice.io[1][i];
                        }
                    }
                },
                |e| eprintln!("output stream: {e}"),
                None,
            )
            .expect("output stream");
        stream.play().expect("start output");
        streams.push(stream);
    }

    let editor = Editor::new(shared);
    let code = unsafe { sp9ceverb::cocoa::run_window("Sp9ceVerb", editor) };
    drop(streams);
    std::process::exit(code);
}
