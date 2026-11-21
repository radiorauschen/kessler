// Where the sound goes, on a Mac: a pause asked for the moment it would start
// coming out of the built-in speakers instead of headphones — AirPods taken
// out, a Bluetooth set switched off, a cable pulled from the jack. What a
// phone does, and what anyone with headphones on expects: the music does not
// go on out loud in the room.
//
// Two ways it gets there. Headphones of their own (Bluetooth, USB) are a
// device of their own, and taking them away changes the default output to
// another, often the speakers. A cable in the jack is the same built-in device
// either way; only its data source changes, from headphones to the internal
// speakers. Both are listened for.
//
// Built on its own, as the menu bar's symbol and the Touch Bar are: a file
// that will not compile costs this and nothing else. The listeners run on a
// thread of CoreAudio's; what they find is kept under a lock until the window
// asks, on its next frame, which they wake it for.

#include <stdlib.h>
#include <CoreAudio/CoreAudio.h>
#include <pthread.h>

// Defined on the Rust side: asks the window for a frame.
extern void kessler_media_woken(void);

// The media keys' number for a pause — see `Command::from_code`.
#define KESSLER_ROUTE_PAUSE 2

static pthread_mutex_t kessler_route_lock = PTHREAD_MUTEX_INITIALIZER;
static int kessler_route_started = 0;
static int kessler_route_pending = 0;
// Whether the sound goes to the built-in speakers: -1 before it was looked at.
static int kessler_route_on_speakers = -1;
static AudioObjectID kessler_route_watched = kAudioObjectUnknown;

// The data source of the built-in output that means its own speakers: the
// four letters „ispk" as CoreAudio packs them.
static const UInt32 KESSLER_ROUTE_INTERNAL_SPEAKER = 0x6973706B;

static AudioObjectPropertyAddress kessler_route_default_address(void) {
    AudioObjectPropertyAddress address = {
        kAudioHardwarePropertyDefaultOutputDevice,
        kAudioObjectPropertyScopeGlobal,
        0,
    };
    return address;
}

static AudioObjectPropertyAddress kessler_route_source_address(void) {
    AudioObjectPropertyAddress address = {
        kAudioDevicePropertyDataSource,
        kAudioDevicePropertyScopeOutput,
        0,
    };
    return address;
}

static AudioObjectID kessler_route_default_output(void) {
    AudioObjectPropertyAddress address = kessler_route_default_address();
    AudioObjectID device = kAudioObjectUnknown;
    UInt32 size = sizeof(device);
    if (AudioObjectGetPropertyData(kAudioObjectSystemObject, &address, 0, NULL, &size, &device) != noErr) {
        return kAudioObjectUnknown;
    }
    return device;
}

// Whether the first output stream of `device` says it ends in headphones —
// on the Macs whose jack is a device of its own. Not known, it does not.
static int kessler_route_streams_to_headphones(AudioObjectID device) {
    AudioObjectPropertyAddress streams = {
        kAudioDevicePropertyStreams,
        kAudioObjectPropertyScopeOutput,
        0,
    };
    UInt32 size = 0;
    if (AudioObjectGetPropertyDataSize(device, &streams, 0, NULL, &size) != noErr || size < sizeof(AudioStreamID)) {
        return 0;
    }
    // The whole list, as the property is asked for; the first is enough.
    AudioStreamID *all = (AudioStreamID *)malloc(size);
    if (all == NULL) {
        return 0;
    }
    if (AudioObjectGetPropertyData(device, &streams, 0, NULL, &size, all) != noErr || size < sizeof(AudioStreamID)) {
        free(all);
        return 0;
    }
    AudioStreamID stream = all[0];
    free(all);
    AudioObjectPropertyAddress terminal = {
        kAudioStreamPropertyTerminalType,
        kAudioObjectPropertyScopeGlobal,
        0,
    };
    UInt32 kind = 0;
    size = sizeof(kind);
    if (AudioObjectGetPropertyData(stream, &terminal, 0, NULL, &size, &kind) != noErr) {
        return 0;
    }
    return kind == kAudioStreamTerminalTypeHeadphones;
}

// Whether `device` is the Mac's own speakers: the built-in output, playing
// through its internal speaker rather than the headphone jack.
static int kessler_route_is_speakers(AudioObjectID device) {
    if (device == kAudioObjectUnknown) {
        return 0;
    }
    AudioObjectPropertyAddress transport = {
        kAudioDevicePropertyTransportType,
        kAudioObjectPropertyScopeGlobal,
        0,
    };
    UInt32 kind = 0;
    UInt32 size = sizeof(kind);
    if (AudioObjectGetPropertyData(device, &transport, 0, NULL, &size, &kind) != noErr) {
        return 0;
    }
    if (kind != kAudioDeviceTransportTypeBuiltIn) {
        return 0;
    }
    AudioObjectPropertyAddress source = kessler_route_source_address();
    if (!AudioObjectHasProperty(device, &source)) {
        // A built-in output without sources to choose from: a Mac with
        // separate devices for speakers and jack. Which one this is, its
        // stream says — the jack's is headphones, the speakers' a speaker.
        return !kessler_route_streams_to_headphones(device);
    }
    UInt32 said = 0;
    size = sizeof(said);
    if (AudioObjectGetPropertyData(device, &source, 0, NULL, &size, &said) != noErr) {
        return 1;
    }
    return said == KESSLER_ROUTE_INTERNAL_SPEAKER;
}

static OSStatus kessler_route_changed(AudioObjectID object, UInt32 count, const AudioObjectPropertyAddress *addresses, void *data);

// Look again where the sound goes; under the lock. A step from anywhere else
// onto the speakers is a pause to be asked for.
static void kessler_route_look(void) {
    AudioObjectID device = kessler_route_default_output();
    if (device != kessler_route_watched) {
        AudioObjectPropertyAddress source = kessler_route_source_address();
        if (kessler_route_watched != kAudioObjectUnknown && AudioObjectHasProperty(kessler_route_watched, &source)) {
            AudioObjectRemovePropertyListener(kessler_route_watched, &source, kessler_route_changed, NULL);
        }
        if (device != kAudioObjectUnknown && AudioObjectHasProperty(device, &source)) {
            AudioObjectAddPropertyListener(device, &source, kessler_route_changed, NULL);
        }
        kessler_route_watched = device;
    }
    int now = kessler_route_is_speakers(device);
    if (kessler_route_on_speakers == 0 && now == 1) {
        kessler_route_pending = 1;
    }
    kessler_route_on_speakers = now;
}

static OSStatus kessler_route_changed(AudioObjectID object, UInt32 count, const AudioObjectPropertyAddress *addresses, void *data) {
    (void)object;
    (void)count;
    (void)addresses;
    (void)data;
    pthread_mutex_lock(&kessler_route_lock);
    kessler_route_look();
    int pending = kessler_route_pending;
    pthread_mutex_unlock(&kessler_route_lock);
    if (pending) {
        kessler_media_woken();
    }
    return noErr;
}

// Begin listening; once, whatever is called after.
void kessler_route_start(void) {
    pthread_mutex_lock(&kessler_route_lock);
    if (!kessler_route_started) {
        kessler_route_started = 1;
        AudioObjectPropertyAddress address = kessler_route_default_address();
        AudioObjectAddPropertyListener(kAudioObjectSystemObject, &address, kessler_route_changed, NULL);
        kessler_route_look();
        // Where it goes at the start is no step.
        kessler_route_pending = 0;
    }
    pthread_mutex_unlock(&kessler_route_lock);
}

// A pause, once, if the sound has gone over to the speakers since the last
// asking; else nothing.
int kessler_route_poll(void) {
    pthread_mutex_lock(&kessler_route_lock);
    int pending = kessler_route_pending;
    kessler_route_pending = 0;
    pthread_mutex_unlock(&kessler_route_lock);
    return pending ? KESSLER_ROUTE_PAUSE : 0;
}
