#import <AppKit/AppKit.h>
#import <Foundation/Foundation.h>
#import <MediaPlayer/MediaPlayer.h>
#import <objc/runtime.h>

enum {
    KESSLER_NONE = 0,
    KESSLER_PLAY = 1,
    KESSLER_PAUSE = 2,
    KESSLER_TOGGLE = 3,
    KESSLER_NEXT = 4,
    KESSLER_PREVIOUS = 5,
    KESSLER_STOP = 6,
};

#define KESSLER_QUEUE 16

static int kessler_queue[KESSLER_QUEUE];
static int kessler_head = 0;
static int kessler_tail = 0;
static NSLock *kessler_lock = nil;
static BOOL kessler_playing = NO;
static BOOL kessler_started = NO;

extern void kessler_media_woken(void);

static void kessler_push(int command) {
    if (kessler_lock == nil) {
        return;
    }
    [kessler_lock lock];
    int next = (kessler_tail + 1) % KESSLER_QUEUE;
    if (next != kessler_head) {
        kessler_queue[kessler_tail] = command;
        kessler_tail = next;
    }
    [kessler_lock unlock];
    kessler_media_woken();
}

@interface KesslerControls : NSObject
- (void)toggle:(id)sender;
- (void)next:(id)sender;
- (void)previous:(id)sender;
@end

@implementation KesslerControls
- (void)toggle:(id)sender {
    (void)sender;
    kessler_push(KESSLER_TOGGLE);
}
- (void)next:(id)sender {
    (void)sender;
    kessler_push(KESSLER_NEXT);
}
- (void)previous:(id)sender {
    (void)sender;
    kessler_push(KESSLER_PREVIOUS);
}
@end

static KesslerControls *kessler_controls = nil;

static NSMenu *kessler_build_menu(void) {
    NSMenu *menu = [[NSMenu alloc] initWithTitle:@"kessler"];
    NSMenuItem *play = [[NSMenuItem alloc]
        initWithTitle:(kessler_playing ? @"Anhalten" : @"Abspielen")
               action:@selector(toggle:)
        keyEquivalent:@""];
    play.target = kessler_controls;
    [menu addItem:play];

    NSMenuItem *next = [[NSMenuItem alloc] initWithTitle:@"Nächster Titel"
                                                  action:@selector(next:)
                                           keyEquivalent:@""];
    next.target = kessler_controls;
    [menu addItem:next];

    NSMenuItem *previous = [[NSMenuItem alloc] initWithTitle:@"Voriger Titel"
                                                      action:@selector(previous:)
                                               keyEquivalent:@""];
    previous.target = kessler_controls;
    [menu addItem:previous];

    return menu;
}

static NSMenu *kessler_dock_menu(id self, SEL _cmd, NSApplication *application) {
    (void)self;
    (void)_cmd;
    (void)application;
    return kessler_build_menu();
}

static void kessler_install_dock_menu(void) {
    id delegate = [NSApp delegate];
    if (delegate == nil) {
        return;
    }
    Class cls = object_getClass(delegate);
    SEL selector = @selector(applicationDockMenu:);
    if (!class_addMethod(cls, selector, (IMP)kessler_dock_menu, "@@:@")) {
        Method existing = class_getInstanceMethod(cls, selector);
        if (existing != NULL) {
            method_setImplementation(existing, (IMP)kessler_dock_menu);
        }
    }
}

void kessler_media_start(void) {
    if (kessler_started) {
        return;
    }
    kessler_started = YES;
    kessler_lock = [[NSLock alloc] init];
    kessler_controls = [[KesslerControls alloc] init];

    MPRemoteCommandCenter *centre = [MPRemoteCommandCenter sharedCommandCenter];

    [centre.playCommand addTargetWithHandler:^MPRemoteCommandHandlerStatus(
        MPRemoteCommandEvent *event) {
        (void)event;
        kessler_push(KESSLER_PLAY);
        return MPRemoteCommandHandlerStatusSuccess;
    }];
    [centre.pauseCommand addTargetWithHandler:^MPRemoteCommandHandlerStatus(
        MPRemoteCommandEvent *event) {
        (void)event;
        kessler_push(KESSLER_PAUSE);
        return MPRemoteCommandHandlerStatusSuccess;
    }];
    [centre.togglePlayPauseCommand addTargetWithHandler:^MPRemoteCommandHandlerStatus(
        MPRemoteCommandEvent *event) {
        (void)event;
        kessler_push(KESSLER_TOGGLE);
        return MPRemoteCommandHandlerStatusSuccess;
    }];
    [centre.nextTrackCommand addTargetWithHandler:^MPRemoteCommandHandlerStatus(
        MPRemoteCommandEvent *event) {
        (void)event;
        kessler_push(KESSLER_NEXT);
        return MPRemoteCommandHandlerStatusSuccess;
    }];
    [centre.previousTrackCommand addTargetWithHandler:^MPRemoteCommandHandlerStatus(
        MPRemoteCommandEvent *event) {
        (void)event;
        kessler_push(KESSLER_PREVIOUS);
        return MPRemoteCommandHandlerStatusSuccess;
    }];
    [centre.stopCommand addTargetWithHandler:^MPRemoteCommandHandlerStatus(
        MPRemoteCommandEvent *event) {
        (void)event;
        kessler_push(KESSLER_STOP);
        return MPRemoteCommandHandlerStatusSuccess;
    }];

    centre.playCommand.enabled = YES;
    centre.pauseCommand.enabled = YES;
    centre.togglePlayPauseCommand.enabled = YES;
    centre.nextTrackCommand.enabled = YES;
    centre.previousTrackCommand.enabled = YES;
    centre.stopCommand.enabled = YES;
    centre.changePlaybackPositionCommand.enabled = NO;
    centre.seekForwardCommand.enabled = NO;
    centre.seekBackwardCommand.enabled = NO;

    kessler_install_dock_menu();
}

int kessler_media_poll(void) {
    if (kessler_lock == nil) {
        return KESSLER_NONE;
    }
    [kessler_lock lock];
    int command = KESSLER_NONE;
    if (kessler_head != kessler_tail) {
        command = kessler_queue[kessler_head];
        kessler_head = (kessler_head + 1) % KESSLER_QUEUE;
    }
    [kessler_lock unlock];
    return command;
}

static NSString *kessler_string(const char *text) {
    if (text == NULL) {
        return @"";
    }
    NSString *string = [NSString stringWithUTF8String:text];
    return string == nil ? @"" : string;
}

static NSString *kessler_artwork_path = nil;
static MPMediaItemArtwork *kessler_artwork = nil;

static MPMediaItemArtwork *kessler_artwork_for(NSString *path) {
    if (path.length == 0) {
        kessler_artwork_path = nil;
        kessler_artwork = nil;
        return nil;
    }
    if (kessler_artwork_path != nil && [kessler_artwork_path isEqualToString:path]) {
        return kessler_artwork;
    }
    kessler_artwork_path = [path copy];
    kessler_artwork = nil;
    NSImage *image = [[NSImage alloc] initWithContentsOfFile:path];
    if (image != nil && image.size.width > 0 && image.size.height > 0) {
        kessler_artwork = [[MPMediaItemArtwork alloc]
            initWithBoundsSize:image.size
                requestHandler:^NSImage *(CGSize size) {
                    (void)size;
                    return image;
                }];
    }
    return kessler_artwork;
}

void kessler_media_update(int state,
                          const char *title,
                          const char *artist,
                          const char *album,
                          double duration,
                          double elapsed,
                          const char *artwork) {
    MPNowPlayingInfoCenter *centre = [MPNowPlayingInfoCenter defaultCenter];
    kessler_playing = (state == 1);

    if (state == 0) {
        kessler_artwork_for(@"");
        centre.nowPlayingInfo = nil;
        centre.playbackState = MPNowPlayingPlaybackStateStopped;
        return;
    }

    NSMutableDictionary *info = [NSMutableDictionary dictionary];
    info[MPMediaItemPropertyTitle] = kessler_string(title);
    info[MPMediaItemPropertyArtist] = kessler_string(artist);
    info[MPMediaItemPropertyAlbumTitle] = kessler_string(album);
    info[MPMediaItemPropertyPlaybackDuration] = @(duration);
    info[MPNowPlayingInfoPropertyElapsedPlaybackTime] = @(elapsed);
    info[MPNowPlayingInfoPropertyPlaybackRate] = @(state == 1 ? 1.0 : 0.0);
    info[MPNowPlayingInfoPropertyMediaType] = @(MPNowPlayingInfoMediaTypeAudio);
    MPMediaItemArtwork *cover = kessler_artwork_for(kessler_string(artwork));
    if (cover != nil) {
        info[MPMediaItemPropertyArtwork] = cover;
    }

    centre.nowPlayingInfo = info;
    centre.playbackState =
        (state == 1) ? MPNowPlayingPlaybackStatePlaying : MPNowPlayingPlaybackStatePaused;
}

void kessler_hide_app(void) {
    [NSApp hide:nil];
}

void kessler_set_dock_icon(const unsigned char *rgba, int width, int height) {
    if (rgba == NULL || width <= 0 || height <= 0) {
        return;
    }
    NSBitmapImageRep *rep = [[NSBitmapImageRep alloc]
        initWithBitmapDataPlanes:NULL
                      pixelsWide:width
                      pixelsHigh:height
                   bitsPerSample:8
                 samplesPerPixel:4
                        hasAlpha:YES
                        isPlanar:NO
                  colorSpaceName:NSDeviceRGBColorSpace
                     bytesPerRow:width * 4
                    bitsPerPixel:32];
    if (rep == nil) {
        return;
    }
    memcpy([rep bitmapData], rgba, (size_t)width * (size_t)height * 4);
    NSImage *image = [[NSImage alloc] initWithSize:NSMakeSize(width, height)];
    [image addRepresentation:rep];
    [NSApp setApplicationIconImage:image];
}
