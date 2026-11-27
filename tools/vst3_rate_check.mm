// Loads a Sp9ceVerb VST3 bundle as a host does and changes the sample rate the
// way a host does before an offline render (deactivate, set up, activate,
// process), many times over. Every call must succeed and every rate must give
// finite output that is not silent. No host, so a failure here is the plugin's.
//
// Build as tools/vst3_editor_check.mm is built in scripts/build_shells.sh, then:
//   ./vst3_rate_check dist/Sp9ceVerb.vst3 [cycles]        exit 0 = pass
#import <Foundation/Foundation.h>
#include "pluginterfaces/base/ipluginbase.h"
#include "pluginterfaces/vst/ivstaudioprocessor.h"
#include "pluginterfaces/vst/ivstcomponent.h"
#include "pluginterfaces/vst/ivsteditcontroller.h"
#include "pluginterfaces/vst/ivstmessage.h"
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <vector>
using namespace Steinberg;
using namespace Steinberg::Vst;
#define CHECK(cond) do { if (!(cond)) { std::printf("FAIL line %d: %s\n", __LINE__, #cond); return 1; } } while (0)

int main(int argc, char** argv) {
    if (argc < 2) {
        std::printf("usage: vst3_rate_check <bundle.vst3> [cycles]\n");
        return 2;
    }
    const int cycles = argc > 2 ? std::atoi(argv[2]) : 50;
    @autoreleasepool {
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
        IConnectionPoint* fromComponent = nullptr;
        IConnectionPoint* fromController = nullptr;
        CHECK(component->queryInterface(IConnectionPoint::iid, reinterpret_cast<void**>(&fromComponent)) == kResultOk);
        CHECK(controller->queryInterface(IConnectionPoint::iid, reinterpret_cast<void**>(&fromController)) == kResultOk);
        CHECK(fromComponent->connect(fromController) == kResultOk && fromController->connect(fromComponent) == kResultOk);
        SpeakerArrangement stereo = SpeakerArr::kStereo;
        CHECK(processor->setBusArrangements(&stereo, 1, &stereo, 1) == kResultOk);

        const double rates[] = {44100, 96000, 192000, 768000, 48000, 22050, 8000};
        const int32 blocks[] = {512, 1024, 64, 4096};
        int failures = 0, runs = 0;
        for (int cycle = 0; cycle < cycles; ++cycle) {
            for (double rate : rates) {
                const int32 block = blocks[(cycle + runs) % 4];
                ++runs;
                ProcessSetup setup{cycle % 2 ? kOffline : kRealtime, kSample32, block, rate};
                const tresult off = component->setActive(false);
                const tresult su = processor->setupProcessing(setup);
                const tresult on = component->setActive(true);
                const tresult sp = processor->setProcessing(true);
                // 0.1 s: a click, then noise, as a render's test signal.
                std::vector<float> inL(block), inR(block), outL(block), outR(block);
                float* in[2] = {inL.data(), inR.data()};
                float* out[2] = {outL.data(), outR.data()};
                AudioBusBuffers inBus{}, outBus{};
                inBus.numChannels = outBus.numChannels = 2;
                inBus.channelBuffers32 = in;
                outBus.channelBuffers32 = out;
                ProcessData data{};
                data.processMode = setup.processMode;
                data.symbolicSampleSize = kSample32;
                data.numSamples = block;
                data.numInputs = data.numOutputs = 1;
                data.inputs = &inBus;
                data.outputs = &outBus;
                const int total = static_cast<int>(rate * 0.1);
                float peak = 0.0f;
                bool finite = true;
                tresult pr = kResultOk;
                uint32 seed = 1;
                for (int done = 0; done < total; done += block) {
                    for (int32 i = 0; i < block; ++i) {
                        seed = seed * 1664525u + 1013904223u;
                        const float x = done + i == 0 ? 1.0f : 0.1f * (static_cast<float>(seed >> 8) / 8388608.0f - 1.0f);
                        inL[i] = inR[i] = x;
                    }
                    const tresult r = processor->process(data);
                    if (r != kResultOk) pr = r;
                    for (int32 i = 0; i < block; ++i) {
                        finite = finite && std::isfinite(outL[i]) && std::isfinite(outR[i]);
                        peak = std::fmax(peak, std::fmax(std::fabs(outL[i]), std::fabs(outR[i])));
                    }
                }
                processor->setProcessing(false);
                const bool ok = off == kResultOk && su == kResultOk && on == kResultOk && pr == kResultOk && finite && peak > 1e-3f;
                (void)sp; // setProcessing may answer kNotImplemented
                if (!ok) {
                    ++failures;
                    std::printf("FAIL cycle %d rate %.0f block %d: setActive(false) %d setup %d setActive(true) %d process %d finite %d peak %g\n",
                                cycle, rate, block, off, su, on, pr, finite, peak);
                }
            }
        }
        component->setActive(false);
        fromComponent->disconnect(fromController);
        fromController->disconnect(fromComponent);
        fromComponent->release();
        fromController->release();
        processor->release();
        component->terminate();
        component->release();
        controller->terminate();
        controller->release();
        std::printf("%d of %d rate changes failed\n", failures, runs);
        if (failures) return 1;
        std::printf("PASS\n");
    }
    return 0;
}
