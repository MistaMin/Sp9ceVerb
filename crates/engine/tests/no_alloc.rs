//! The process path must not allocate.
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use sp9ce_engine::{params, Engine, Mode, ParamId};

struct Counting;
static ALLOCS: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

#[test]
fn process_and_parameter_changes_do_not_allocate() {
    let input = vec![0.25f32; 512];
    let mut out_l = vec![0.0f32; 512];
    let mut out_r = vec![0.0f32; 512];
    for m in Mode::ALL {
        let mut e = Engine::new(m);
        let specs: Vec<_> = params::specs(m).collect();
        let before = ALLOCS.load(Ordering::Relaxed);
        for round in 0..40 {
            for s in &specs {
                if s.id != ParamId::Mode {
                    e.set_param(s.id, if round % 2 == 0 { s.max } else { s.min });
                }
            }
            e.process(&input, &input, &mut out_l, &mut out_r);
        }
        assert_eq!(ALLOCS.load(Ordering::Relaxed), before, "{m:?} allocated on the audio path");
    }
}
