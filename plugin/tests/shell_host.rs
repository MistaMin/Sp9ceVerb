//! Drives the ABI the VST3, Audio Unit and AAX shells link, the way a shell does.
use sp9ce_engine::ParamId;
use sp9ceverb::plugin::info;
use sp9ceverb::shell_host::*;

#[test]
fn an_instance_is_tied_to_its_shared_state_until_it_is_destroyed() {
    unsafe {
        let a = op_shell_create(48_000.0, 512, 2);
        let b = op_shell_create(48_000.0, 512, 2);
        assert!(!a.is_null() && !b.is_null());
        let (sa, sb) = (shared_for(a).unwrap(), shared_for(b).unwrap());
        assert!(!std::sync::Arc::ptr_eq(&sa, &sb), "each instance has its own state");

        // A value set on one instance reaches its shared state once audio runs, and only its own.
        let index = ParamId::ReverbTime.index() as u32;
        assert_eq!(op_shell_set_plain(a, index, 5.0), 1);
        let input = [0.0f32; 512];
        let (mut l, mut r) = ([0.0f32; 512], [0.0f32; 512]);
        assert_eq!(op_shell_process(a, input.as_ptr(), input.as_ptr(), l.as_mut_ptr(), r.as_mut_ptr(), 512), 1);
        assert_eq!(sa.value(index as usize), 5.0);
        assert_eq!(sb.value(index as usize), info().params[index as usize].default);

        op_shell_destroy(a);
        assert!(shared_for(a).is_none());
        assert!(shared_for(b).is_some());
        op_shell_destroy(b);
    }
}

#[test]
fn tail_follows_the_decay_the_echoes_infinite_and_chaos() {
    unsafe {
        let p = op_shell_create(48_000.0, 256, 2);
        let input = [0.0f32; 256];
        let (mut l, mut r) = ([0.0f32; 256], [0.0f32; 256]);
        let mut run = || op_shell_process(p, input.as_ptr(), input.as_ptr(), l.as_mut_ptr(), r.as_mut_ptr(), 256);
        let set = |id: ParamId, v: f32| op_shell_set_plain(p, id.index() as u32, v);

        // Hall: reverb time in the low band, plus the margin.
        set(ParamId::ReverbTime, 3.0);
        set(ParamId::BassRt, 2.0);
        run();
        assert!((op_shell_tail_seconds(p) - 7.5).abs() < 1e-3);

        // Infinite and the echoes only exist in the plate and chamber voicings.
        set(ParamId::Infinite, 1.0);
        set(ParamId::EchoLTime, 1.0);
        set(ParamId::EchoLFb, 0.9);
        set(ParamId::EchoRFb, 0.0);
        run();
        assert!((op_shell_tail_seconds(p) - 7.5).abs() < 1e-3, "Hall has neither");
        set(ParamId::Mode, 2.0);
        run();
        assert!(op_shell_tail_seconds(p).is_infinite(), "Infinite holds the plate");

        // A one-second echo at 0.9 feedback needs about 65.6 repeats to fall 60 dB.
        set(ParamId::Infinite, 0.0);
        run();
        let tail = op_shell_tail_seconds(p);
        assert!((tail - (65.56 + 7.5)).abs() < 0.1, "{tail}");
        set(ParamId::EchoLFb, 0.0);
        run();
        assert!((op_shell_tail_seconds(p) - 7.5).abs() < 1e-3, "no feedback, no echo tail");

        // Chaos sustains itself as soon as it is above zero.
        set(ParamId::Mode, 8.0);
        run();
        assert!(op_shell_tail_seconds(p).is_finite(), "Chaos at zero is the plain reverb");
        set(ParamId::Chaos, 0.2);
        run();
        assert!(op_shell_tail_seconds(p).is_infinite());
        op_shell_destroy(p);

        // An unknown instance gets the longest finite tail the ranges allow.
        let longest = op_shell_tail_seconds(std::ptr::null_mut());
        assert!(longest.is_finite() && longest > 240.0 && longest < 250.0, "{longest}");
    }
}

#[cfg(target_os = "macos")]
#[test]
fn editor_reports_size_and_tolerates_null() {
    unsafe {
        let (mut w, mut h) = (0u32, 0u32);
        assert_eq!(op_editor_size(&mut w, &mut h), 1);
        assert!(w > 100 && h > 100);
        assert_eq!(op_editor_size(std::ptr::null_mut(), &mut h), 0);
        assert!(op_editor_view(std::ptr::null_mut()).is_null());
        assert_eq!(op_editor_attach(std::ptr::null_mut(), std::ptr::null_mut()), 0);
        op_editor_bind(std::ptr::null_mut(), std::ptr::null_mut());
        op_editor_set_plain(std::ptr::null_mut(), 0, 1.0);
        op_editor_destroy(std::ptr::null_mut());
    }
}

#[test]
fn tail_covers_the_reversed_room_pattern() {
    unsafe {
        let p = op_shell_create(48_000.0, 256, 2);
        let input = [0.0f32; 256];
        let (mut l, mut r) = ([0.0f32; 256], [0.0f32; 256]);
        let set = |id: ParamId, v: f32| op_shell_set_plain(p, id.index() as u32, v);
        set(ParamId::Mode, 0.0);
        set(ParamId::ReverbTime, 0.1);
        set(ParamId::RoomScale, 4.0);
        op_shell_process(p, input.as_ptr(), input.as_ptr(), l.as_mut_ptr(), r.as_mut_ptr(), 256);
        assert!((op_shell_tail_seconds(p) - 1.6).abs() < 1e-3);
        // Reversed, the pattern is 0.5 s x Room Scale long and loudest at its end.
        set(ParamId::Reverse, 1.0);
        op_shell_process(p, input.as_ptr(), input.as_ptr(), l.as_mut_ptr(), r.as_mut_ptr(), 256);
        assert!((op_shell_tail_seconds(p) - 3.6).abs() < 1e-3);
        op_shell_destroy(p);
    }
}
