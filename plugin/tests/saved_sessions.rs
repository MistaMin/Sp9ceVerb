//! Sessions saved by released versions must keep opening with every value.

use openplugin::prelude::Plugin;
use sp9ceverb::plugin::{info, OnePlugin};

/// The state a 1.0.1-Beta CLAP saved in a host session, every control moved.
const STATE_1_0_1: &str = "T1BTVAEANgAAAIBAUrgeP6FKzTzhCyM/AAAAQOXQ7kCk2JlEkZsdQDMx3EU1Xlo/fh0YPwAAoEAAAIA/AACAPzvfS0Do+6k9TKuhPbFQyz5SSf0+exQuv5JcLj/oais//7I7P9xGwz57wjI+LpAAPqftHz6sW709PBQFPyuHFj7/PuM+BFZuPgAAAAAAAIA/AACAP+58Tz8QWDk/PpKSPS2yvT4AAIA/AACAPxhZsj1miOM+AAAAQAAAgD+oxnM/q89VPpvmHT85RQc+r5SFPnzQaz/ecSq/Vg4dPwAAgD8=";

const VALUES_1_0_1: [f32; 54] = [
    4.0, 0.62, 0.02506, 0.6369, 2.0, 7.463, 1230.77, 2.46262, 7046.15, 0.853, 0.5942, 5.0, 1.0, 1.0, 3.1855, 0.083,
    0.07894, 0.3971, 0.4947, -0.68, 0.6811, 0.6696, 0.7332, 0.3814, 0.17457, 0.12555, 0.15618, 0.09246, 0.51984, 0.147,
    0.44384, 0.23275, 0.0, 1.0, 1.0, 0.8105, 0.724, 0.071568, 0.3705, 1.0, 1.0, 0.087084, 0.4444, 2.0, 1.0, 0.95225,
    0.2088, 0.6168, 0.1321, 0.2609, 0.92115, -0.6658, 0.6135, 1.0,
];

/// Hosts and saved states find parameters by position: append, never insert or reorder.
const IDS: [&str; 54] = [
    "mode", "mix", "predelay", "diffusion", "shape", "reverb_time", "bass_xover", "bass_rt", "hf_damp", "width", "level",
    "pattern", "reverse", "randomize", "room_scale", "early_predelay", "tail_predelay", "spin", "wander", "tap_slope",
    "ref_1_level", "ref_2_level", "ref_3_level", "ref_4_level", "ref_1_delay", "ref_2_delay", "ref_3_delay",
    "ref_4_delay", "echo_l_time", "echo_r_time", "echo_l_fb", "echo_r_fb", "infinite", "chorus", "chorus_type",
    "chorus_depth", "definition", "ref_l_delay", "ref_l_gain", "ref_l_mode", "ref_l_polarity", "ref_r_delay",
    "ref_r_gain", "ref_r_mode", "ref_r_polarity", "amb_size", "amb_tone", "amb_character", "chaos", "chaos_drive",
    "chaos_gain", "chaos_shift", "chaos_scramble", "chaos_nonlin",
];

fn base64(text: &str) -> Vec<u8> {
    let digit = |c: u8| match c {
        b'A'..=b'Z' => c - b'A',
        b'a'..=b'z' => c - b'a' + 26,
        b'0'..=b'9' => c - b'0' + 52,
        b'+' => 62,
        _ => 63,
    };
    let mut out = Vec::new();
    for chunk in text.trim_end_matches('=').as_bytes().chunks(4) {
        let n = chunk.iter().enumerate().fold(0u32, |n, (i, &c)| n | (digit(c) as u32) << (18 - 6 * i));
        out.extend_from_slice(&n.to_be_bytes()[1..chunk.len()]);
    }
    out
}

#[test]
fn parameter_order_is_the_released_one() {
    let ids: Vec<&str> = info().params.iter().map(|p| p.id).collect();
    assert_eq!(ids, IDS);
}

#[test]
fn a_1_0_1_session_opens_with_every_value() {
    let mut plugin = OnePlugin::default();
    assert!(plugin.load_state(&base64(STATE_1_0_1)));
    for (id, want) in IDS.iter().zip(VALUES_1_0_1) {
        let got = plugin.param(id).unwrap();
        assert!((got - want).abs() <= 1e-4 * want.abs().max(1.0), "{id}: {got} != {want}");
    }
}
