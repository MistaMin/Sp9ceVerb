//! The SDK's conformance kit on the plugin. Debug builds run Options::quick();
//! release builds run every rate and block size.

use openplugin::testing::{conformance, Options};
use sp9ceverb::plugin::OnePlugin;

openplugin::testing::install_allocator!();

#[test]
fn conforms() {
    let options = if cfg!(debug_assertions) { Options::quick() } else { Options::default() };
    let report = conformance(OnePlugin::default, options);
    println!("{report}");
    report.assert_pass();
}
