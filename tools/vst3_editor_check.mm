// Loads a Sp9ceVerb VST3 bundle as a host does, opens its editor in a window
// that is never shown, clicks "next preset", and checks that the controller
// reported the change to the host with paired begin/perform/end edits. It also
// activates and deactivates the processor with the editor open (the editor is
// re-bound to the engine each time) and reads the tail length.
//
// Build against the VST3 SDK and the libraries scripts/build_shells.sh leaves
// in plugin/target/shells/lib/Release (see AGENTS.md), then:
//   ./vst3_editor_check dist/Sp9ceVerb.vst3        exit 0 = pass
#import <Cocoa/Cocoa.h>
#include "pluginterfaces/base/ipluginbase.h"
#include "pluginterfaces/gui/iplugview.h"
#include "pluginterfaces/vst/ivstaudioprocessor.h"
#include "pluginterfaces/vst/ivstcomponent.h"
#include "pluginterfaces/vst/ivsteditcontroller.h"
#include "pluginterfaces/vst/ivstmessage.h"
#include <cstdio>
#include <cstring>
#include <initializer_list>
using namespace Steinberg;
using namespace Steinberg::Vst;
#define CHECK(cond) do { if (!(cond)) { std::printf("FAIL line %d: %s\n", __LINE__, #cond); return 1; } } while (0)

// The host side of parameter editing: counts what the controller reports.
struct Handler final : IComponentHandler {
    int begins = 0, performs = 0, ends = 0;
    double reverbTime = -1.0;
    tresult PLUGIN_API beginEdit(ParamID) override { ++begins; return kResultOk; }
    tresult PLUGIN_API performEdit(ParamID id, ParamValue value) override {
        ++performs;
        if (id == 5) reverbTime = value;
        return kResultOk;
    }
    tresult PLUGIN_API endEdit(ParamID) override { ++ends; return kResultOk; }
    tresult PLUGIN_API restartComponent(int32) override { return kResultOk; }
    tresult PLUGIN_API queryInterface(const TUID iid, void** obj) override {
        if (FUnknownPrivate::iidEqual(iid, IComponentHandler::iid) || FUnknownPrivate::iidEqual(iid, FUnknown::iid)) {
            *obj = this;
            return kResultOk;
        }
        *obj = nullptr;
        return kNoInterface;
    }
    uint32 PLUGIN_API addRef() override { return 1; }
    uint32 PLUGIN_API release() override { return 1; }
};

static void spin(double seconds) { CFRunLoopRunInMode(kCFRunLoopDefaultMode, seconds, false); }

int main(int argc, char** argv) {
    if (argc < 2) {
        std::printf("usage: vst3_editor_check <bundle.vst3>\n");
        return 2;
    }
    @autoreleasepool {
        [NSApplication sharedApplication];
        NSBundle* bundle = [NSBundle bundleWithPath:[NSString stringWithUTF8String:argv[1]]];
        CHECK(bundle && [bundle load]);
        CFBundleRef cf = CFBundleCreate(nullptr, (CFURLRef)[bundle bundleURL]);
        auto entry = reinterpret_cast<bool (*)(CFBundleRef)>(CFBundleGetFunctionPointerForName(cf, CFSTR("bundleEntry")));
        auto getFactory = reinterpret_cast<IPluginFactory* (*)()>(CFBundleGetFunctionPointerForName(cf, CFSTR("GetPluginFactory")));
        CHECK(entry && getFactory && entry(cf));
        IPluginFactory* factory = getFactory();
        CHECK(factory);
        PClassInfo effect{};
        bool found = false;
        for (int32 i = 0; i < factory->countClasses(); ++i) {
            PClassInfo info{};
            if (factory->getClassInfo(i, &info) == kResultOk && std::strcmp(info.category, kVstAudioEffectClass) == 0) {
                effect = info;
                found = true;
            }
        }
        CHECK(found);
        IComponent* component = nullptr;
        CHECK(factory->createInstance(effect.cid, IComponent::iid, reinterpret_cast<void**>(&component)) == kResultOk && component);
        CHECK(component->initialize(nullptr) == kResultOk);
        IAudioProcessor* processor = nullptr;
        CHECK(component->queryInterface(IAudioProcessor::iid, reinterpret_cast<void**>(&processor)) == kResultOk);
        TUID controllerId{};
        CHECK(component->getControllerClassId(controllerId) == kResultOk);
        IEditController* controller = nullptr;
        CHECK(factory->createInstance(controllerId, IEditController::iid, reinterpret_cast<void**>(&controller)) == kResultOk && controller);
        CHECK(controller->initialize(nullptr) == kResultOk);
        Handler handler;
        CHECK(controller->setComponentHandler(&handler) == kResultOk);

        // Connect the two halves, as a host does. The editor learns its engine through this.
        IConnectionPoint* fromComponent = nullptr;
        IConnectionPoint* fromController = nullptr;
        CHECK(component->queryInterface(IConnectionPoint::iid, reinterpret_cast<void**>(&fromComponent)) == kResultOk);
        CHECK(controller->queryInterface(IConnectionPoint::iid, reinterpret_cast<void**>(&fromController)) == kResultOk);
        CHECK(fromComponent->connect(fromController) == kResultOk && fromController->connect(fromComponent) == kResultOk);

        SpeakerArrangement stereo = SpeakerArr::kStereo;
        CHECK(processor->setBusArrangements(&stereo, 1, &stereo, 1) == kResultOk);
        ProcessSetup setup{kRealtime, kSample32, 512, 48000.0};
        CHECK(processor->setupProcessing(setup) == kResultOk);
        CHECK(component->setActive(true) == kResultOk);
        const uint32 tail = processor->getTailSamples();
        std::printf("tail %u samples\n", tail);
        CHECK(tail > 48000 && tail != kInfiniteTail);

        IPlugView* view = controller->createView(ViewType::kEditor);
        CHECK(view);
        CHECK(view->isPlatformTypeSupported(kPlatformTypeNSView) == kResultTrue);
        ViewRect rect{};
        CHECK(view->getSize(&rect) == kResultOk && rect.getWidth() > 100 && rect.getHeight() > 100);
        std::printf("editor %dx%d\n", rect.getWidth(), rect.getHeight());
        NSRect frame = NSMakeRect(0, 0, rect.getWidth(), rect.getHeight());
        NSWindow* window = [[NSWindow alloc] initWithContentRect:frame styleMask:NSWindowStyleMaskBorderless backing:NSBackingStoreBuffered defer:NO];
        NSView* parent = [window contentView];
        CHECK(view->attached(parent, kPlatformTypeNSView) == kResultOk);
        CHECK(parent.subviews.count == 1);
        NSView* editor = parent.subviews[0];
        spin(0.2);

        // The engine is destroyed and made again with the editor open.
        CHECK(component->setActive(false) == kResultOk);
        spin(0.2);
        CHECK(component->setActive(true) == kResultOk);
        spin(0.2);

        // "Next preset": the right arrow of the preset bar, 84 points from the top.
        NSPoint at = NSMakePoint(670, frame.size.height - 84);
        for (NSEventType type : {NSEventTypeLeftMouseDown, NSEventTypeLeftMouseUp}) {
            NSEvent* click = [NSEvent mouseEventWithType:type location:at modifierFlags:0 timestamp:0 windowNumber:window.windowNumber context:nil eventNumber:0 clickCount:1 pressure:1];
            if (type == NSEventTypeLeftMouseDown) [editor mouseDown:click];
            else [editor mouseUp:click];
        }
        spin(0.2);
        const double plain = controller->normalizedParamToPlain(5, handler.reverbTime);
        std::printf("edits %d begin / %d perform / %d end, reverb time now %.3f s (controller says %.3f)\n", handler.begins, handler.performs, handler.ends, plain,
                    controller->normalizedParamToPlain(5, controller->getParamNormalized(5)));
        CHECK(handler.begins > 0 && handler.begins == handler.performs && handler.performs == handler.ends);
        CHECK(plain > 7.4 && plain < 7.6); // Cathedral
        CHECK(controller->getParamNormalized(5) == handler.reverbTime);

        CHECK(view->removed() == kResultOk);
        CHECK(parent.subviews.count == 0);
        view->release();
        // Open and close once more, then tear down with the processor going first.
        view = controller->createView(ViewType::kEditor);
        CHECK(view && view->attached(parent, kPlatformTypeNSView) == kResultOk);
        CHECK(component->setActive(false) == kResultOk);
        fromComponent->disconnect(fromController);
        fromController->disconnect(fromComponent);
        fromComponent->release();
        fromController->release();
        processor->release();
        CHECK(component->terminate() == kResultOk);
        component->release();
        spin(0.3);
        CHECK(view->removed() == kResultOk);
        view->release();
        CHECK(controller->terminate() == kResultOk);
        controller->release();
        [window release];
        std::printf("PASS\n");
    }
    return 0;
}
