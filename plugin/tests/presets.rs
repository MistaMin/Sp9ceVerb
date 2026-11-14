//! The embedded presets and the editor's preset bar, driven without a window.
use sp9ce_engine::{Mode, ParamId};
use sp9ceverb::gui::Editor;
use sp9ceverb::plugin::{info, GuiEvent, Shared};
use sp9ceverb::presets;
use std::cell::RefCell;
use std::rc::Rc;

#[test]
fn every_preset_file_is_embedded_and_ordered_by_space() {
    let all = presets::all();
    assert!(presets::embedded() >= Mode::COUNT);
    assert_eq!(all.len(), presets::embedded(), "an embedded preset failed to parse");
    assert!(all.windows(2).all(|w| w[0].mode.index() <= w[1].mode.index()));
}

#[test]
fn choosing_a_preset_sends_each_changed_parameter_as_a_gesture() {
    let shared = Shared::new();
    let mut editor = Editor::new(shared.clone());
    let events = Rc::new(RefCell::new(Vec::new()));
    let sink = events.clone();
    editor.sink = Some(Box::new(move |e| sink.borrow_mut().push(e)));

    let all = presets::all();
    let index = all.iter().position(|p| p.name == "Stone Church").unwrap();
    editor.apply_preset(index);

    // The shared values are the preset's, and the mode went with them.
    for (i, p) in info().params.iter().enumerate() {
        assert_eq!(shared.value(i), p.clamp(all[index].values[i]), "{}", p.name);
    }
    assert_eq!(shared.value(ParamId::ReverbTime.index()), 3.5);

    // Every value is wrapped in its own begin and end, and nothing unchanged was sent.
    let sent = events.borrow().clone();
    assert!(!sent.is_empty() && sent.len() % 3 == 0);
    for g in sent.chunks(3) {
        match (g[0], g[1], g[2]) {
            (GuiEvent::Begin(a), GuiEvent::Value(b, v), GuiEvent::End(c)) => {
                assert!(a == b && b == c);
                assert_ne!(v, info().params[a].default, "{} was sent although unchanged", info().params[a].name);
            }
            other => panic!("not a gesture: {other:?}"),
        }
    }
    // Choosing it again changes nothing.
    editor.apply_preset(index);
    assert_eq!(events.borrow().len(), sent.len());
}

#[test]
fn the_bar_steps_through_presets_and_the_browser_loads_the_one_clicked() {
    let shared = Shared::new();
    let mut editor = Editor::new(shared.clone());
    let all = presets::all();
    let mode = |s: &Shared| s.value(ParamId::Mode.index()).round() as usize;

    // Next, from no preset: the first preset of the current space (Hall).
    editor.mouse_down(670.0, 84.0, 1);
    let first_hall = all.iter().position(|p| p.mode == Mode::Hall).unwrap();
    assert_eq!(shared.preset.load(std::sync::atomic::Ordering::Relaxed) as usize, first_hall + 1);
    editor.tick(0.03);

    // Previous: the last preset of the space before it.
    editor.mouse_down(362.0, 84.0, 1);
    editor.tick(0.03);
    assert_eq!(mode(&shared), all[first_hall - 1].mode.index());

    // Open the browser and click the first entry of the first column.
    editor.mouse_down(500.0, 84.0, 1);
    editor.mouse_down(60.0, 56.0 + 66.0 + 32.0 + 10.0, 1);
    editor.tick(0.03);
    assert_eq!(shared.preset.load(std::sync::atomic::Ordering::Relaxed), 1);
    assert_eq!(mode(&shared), all[0].mode.index());

    // With the browser closed again, the same spot is not a preset.
    editor.mouse_down(60.0, 56.0 + 66.0 + 32.0 + 10.0, 1);
    assert_eq!(shared.preset.load(std::sync::atomic::Ordering::Relaxed), 1);
}
