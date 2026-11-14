// Opens the installed Sp9ceVerb Audio Unit and its Cocoa editor in a window
// that is never shown, clicks "next preset" in the editor, and checks that the
// Audio Unit's parameters changed and that the host was told with gestures.
// Then disposes the Audio Unit while the view is still alive.
//
//   clang++ -std=c++17 -fno-objc-arc tools/au_editor_check.mm -o au_editor_check \
//     -framework Cocoa -framework AudioToolbox -framework AudioUnit
//   ./au_editor_check            exit 0 = pass
#import <AudioToolbox/AudioToolbox.h>
#import <AudioUnit/AUCocoaUIView.h>
#import <Cocoa/Cocoa.h>
#include <cstdio>
#include <initializer_list>

static int begins = 0, ends = 0, values = 0;
static void listener(void*, void*, const AudioUnitEvent* event, UInt64, AudioUnitParameterValue) {
    if (event->mEventType == kAudioUnitEvent_BeginParameterChangeGesture) ++begins;
    if (event->mEventType == kAudioUnitEvent_EndParameterChangeGesture) ++ends;
    if (event->mEventType == kAudioUnitEvent_ParameterValueChange) ++values;
}
static int fail(const char* what) {
    std::printf("FAIL %s\n", what);
    return 1;
}
static void spin(double seconds) { CFRunLoopRunInMode(kCFRunLoopDefaultMode, seconds, false); }

int main() {
    @autoreleasepool {
        [NSApplication sharedApplication];
        AudioComponentDescription want = {'aufx', 'Sp9v', 'OpGd', 0, 0};
        AudioComponent component = AudioComponentFindNext(nullptr, &want);
        if (!component) return fail("the Audio Unit is not installed");
        AudioUnit unit = nullptr;
        if (AudioComponentInstanceNew(component, &unit) != noErr || !unit) return fail("instantiate");
        if (AudioUnitInitialize(unit) != noErr) return fail("initialize");

        AudioUnitCocoaViewInfo info{};
        UInt32 size = sizeof(info);
        if (AudioUnitGetProperty(unit, kAudioUnitProperty_CocoaUI, kAudioUnitScope_Global, 0, &info, &size) != noErr)
            return fail("no Cocoa view property");
        NSBundle* bundle = [NSBundle bundleWithURL:(NSURL*)info.mCocoaAUViewBundleLocation];
        Class factoryClass = [bundle classNamed:(NSString*)info.mCocoaAUViewClass[0]];
        if (!factoryClass) return fail("view class not found in the bundle");
        id<AUCocoaUIBase> factory = [[[factoryClass alloc] init] autorelease];
        NSView* view = [[factory uiViewForAudioUnit:unit withSize:NSMakeSize(0, 0)] retain];
        if (!view || view.subviews.count != 1) return fail("no editor view");
        NSView* editor = view.subviews[0];
        std::printf("editor %.0fx%.0f\n", view.frame.size.width, view.frame.size.height);

        // In a window, so the view's timer runs; the window is never ordered in.
        NSWindow* window = [[NSWindow alloc] initWithContentRect:view.frame styleMask:NSWindowStyleMaskBorderless backing:NSBackingStoreBuffered defer:NO];
        [window setContentView:view];

        AUEventListenerRef ref = nullptr;
        AUEventListenerCreate(listener, nullptr, CFRunLoopGetCurrent(), kCFRunLoopDefaultMode, 0.01, 0.01, &ref);
        for (AudioUnitParameterID id = 0; id < 54; ++id) {
            for (AudioUnitEventType type : {kAudioUnitEvent_BeginParameterChangeGesture, kAudioUnitEvent_EndParameterChangeGesture, kAudioUnitEvent_ParameterValueChange}) {
                AudioUnitEvent event{};
                event.mEventType = type;
                event.mArgument.mParameter = {unit, id, kAudioUnitScope_Global, 0};
                AUEventListenerAddEventType(ref, nullptr, &event);
            }
        }
        spin(0.3);

        AudioUnitParameterValue before = 0, after = 0, mode = -1;
        AudioUnitGetParameter(unit, 5, kAudioUnitScope_Global, 0, &before); // Reverb Time
        // "Next preset": the right arrow of the preset bar, 84 points from the top.
        NSPoint at = NSMakePoint(670, view.frame.size.height - 84);
        for (NSEventType type : {NSEventTypeLeftMouseDown, NSEventTypeLeftMouseUp}) {
            NSEvent* click = [NSEvent mouseEventWithType:type location:at modifierFlags:0 timestamp:0 windowNumber:window.windowNumber context:nil eventNumber:0 clickCount:1 pressure:1];
            if (type == NSEventTypeLeftMouseDown) [editor mouseDown:click];
            else [editor mouseUp:click];
        }
        spin(0.5);
        AudioUnitGetParameter(unit, 5, kAudioUnitScope_Global, 0, &after);
        AudioUnitGetParameter(unit, 0, kAudioUnitScope_Global, 0, &mode);
        std::printf("reverb time %.3f -> %.3f s, mode %.0f, gestures %d begin / %d end, %d value changes\n", before, after, mode, begins, ends, values);
        if (after == before) return fail("the click did not change the Audio Unit's parameters");
        if (begins == 0 || begins != ends) return fail("gestures were not reported in pairs");
        if (values < begins) return fail("fewer value changes than gestures");

        // The host may dispose the Audio Unit before the view. The view must survive that.
        AUListenerDispose(ref);
        AudioUnitUninitialize(unit);
        AudioComponentInstanceDispose(unit);
        spin(0.4);
        [window setContentView:nil];
        [view release];
        [window release];
        spin(0.1);
        std::printf("PASS\n");
    }
    return 0;
}
