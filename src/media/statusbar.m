#import <AppKit/AppKit.h>
#import <Foundation/Foundation.h>

enum {
    KESSLER_TRAY_NONE = 0,
    KESSLER_TRAY_TOGGLE = 3,
    KESSLER_TRAY_NEXT = 4,
    KESSLER_TRAY_PREVIOUS = 5,
    KESSLER_TRAY_SLEEP = 7,
};

#define KESSLER_TRAY_QUEUE 16

static int tray_queue[KESSLER_TRAY_QUEUE];
static int tray_head = 0;
static int tray_tail = 0;
static NSLock *tray_lock = nil;

static NSStatusItem *tray_item = nil;

static NSButton *tray_play_key = nil;

static NSMenu *tray_open_menu = nil;
static NSMenuItem *tray_about_item = nil;

static NSMenuItem *tray_sleep_item = nil;
static NSButton *tray_sleep_key = nil;

static NSImage *tray_cover = nil;
static int tray_cover_at = 0;

static BOOL tray_playing = NO;
static NSString *tray_title = @"";
static NSString *tray_label_play = @"play";
static NSString *tray_label_pause = @"pause";
static NSString *tray_label_next = @"nächster titel";
static NSString *tray_label_previous = @"voriger titel";
static NSString *tray_label_show = @"kessler zeigen";
static NSString *tray_label_quit = @"beenden";
static NSString *tray_label_volume = @"vol";
static NSString *tray_label_sleep = @"";
static int tray_volume = 70;
static int tray_volume_set = -1;

extern void kessler_media_woken(void);

static void tray_push(int command) {
    if (tray_lock == nil) {
        return;
    }
    [tray_lock lock];
    int next = (tray_tail + 1) % KESSLER_TRAY_QUEUE;
    if (next != tray_head) {
        tray_queue[tray_tail] = command;
        tray_tail = next;
    }
    [tray_lock unlock];
    kessler_media_woken();
}

static NSString *tray_string(const char *text) {
    if (text == NULL) {
        return @"";
    }
    NSString *string = [NSString stringWithUTF8String:text];
    return string == nil ? @"" : string;
}

@interface KesslerTray : NSObject <NSMenuDelegate>
- (void)clicked:(id)sender;
- (void)toggle:(id)sender;
- (void)next:(id)sender;
- (void)previous:(id)sender;
- (void)volume:(id)sender;
- (void)sleepStep:(id)sender;
- (void)show:(id)sender;
- (void)quit:(id)sender;
@end

static KesslerTray *tray_target = nil;

static NSMenuItem *tray_entry(NSString *title, SEL action) {
    NSMenuItem *item = [[NSMenuItem alloc] initWithTitle:title action:action keyEquivalent:@""];
    item.target = tray_target;
    return item;
}

static const CGFloat TRAY_WIDE = 220.0;

static const CGFloat TRAY_SIDE = 18.0;

static const CGFloat TRAY_MENU_TOP = 5.0;

static const CGFloat TRAY_KEY_PAD = 5.0;

static void tray_dress(NSButton *key, NSString *word) {
    NSDictionary *look = @{
        NSFontAttributeName : [NSFont menuFontOfSize:0],
        NSForegroundColorAttributeName : [NSColor labelColor],
    };
    key.attributedTitle = [[NSAttributedString alloc] initWithString:word attributes:look];
}

static NSButton *tray_key(NSString *word, NSString *tip, SEL action) {
    NSButton *key = [NSButton buttonWithTitle:word target:tray_target action:action];
    key.bezelStyle = NSBezelStyleRecessed;
    key.showsBorderOnlyWhileMouseInside = YES;
    key.toolTip = tip;
    tray_dress(key, word);
    [key sizeToFit];
    return key;
}

static CGFloat tray_key_wide(NSButton *key, NSString *word) {
    tray_dress(key, word);
    return ceil(key.attributedTitle.size.width) + 2.0 * TRAY_KEY_PAD;
}

static void tray_dress_play(NSButton *key) {
    tray_dress(key, tray_playing ? tray_label_pause : tray_label_play);
}

static NSMenuItem *tray_item_with(NSView *view) {
    NSMenuItem *item = [[NSMenuItem alloc] initWithTitle:@"" action:nil keyEquivalent:@""];
    item.view = view;
    return item;
}

static NSMenuItem *tray_controls(void) {
    const CGFloat tall = 30.0;
    const CGFloat apart = 2.0;
    NSView *row = [[NSView alloc] initWithFrame:NSMakeRect(0, 0, TRAY_WIDE, tall)];
    BOOL arrows = tray_label_next.length > 0;
    NSButton *play = tray_key(@"", @"", @selector(toggle:));
    CGFloat playing_wide = MAX(tray_key_wide(play, tray_label_play), tray_key_wide(play, tray_label_pause));
    tray_dress_play(play);
    NSMutableArray<NSButton *> *keys = [NSMutableArray array];
    NSMutableArray<NSNumber *> *widths = [NSMutableArray array];
    if (arrows) {
        NSButton *back = tray_key(@"←", tray_label_previous, @selector(previous:));
        [keys addObject:back];
        [widths addObject:@(tray_key_wide(back, @"←"))];
    }
    [keys addObject:play];
    [widths addObject:@(playing_wide)];
    if (arrows) {
        NSButton *on = tray_key(@"→", tray_label_next, @selector(next:));
        [keys addObject:on];
        [widths addObject:@(tray_key_wide(on, @"→"))];
    }
    CGFloat x = TRAY_SIDE - TRAY_KEY_PAD;
    for (NSUInteger i = 0; i < keys.count; i++) {
        CGFloat wide = widths[i].doubleValue;
        CGFloat h = keys[i].frame.size.height;
        keys[i].frame = NSMakeRect(x, floor((tall - h) / 2.0), wide, h);
        [row addSubview:keys[i]];
        x += wide + apart;
    }
    tray_play_key = play;
    x += 12.0 - apart - TRAY_KEY_PAD;
    NSTextField *word = [NSTextField labelWithString:tray_label_volume];
    NSDictionary *look = @{
        NSFontAttributeName : [NSFont menuFontOfSize:0],
        NSForegroundColorAttributeName : [NSColor secondaryLabelColor],
    };
    word.attributedStringValue = [[NSAttributedString alloc] initWithString:tray_label_volume attributes:look];
    [word sizeToFit];
    NSSize size = word.frame.size;
    word.frame = NSMakeRect(x, floor((tall - size.height) / 2.0), size.width, size.height);
    [row addSubview:word];
    NSSlider *slider = [NSSlider sliderWithValue:tray_volume minValue:0 maxValue:100 target:tray_target action:@selector(volume:)];
    slider.continuous = YES;
    slider.controlSize = NSControlSizeSmall;
    CGFloat from = x + size.width + 6.0;
    CGFloat h = slider.frame.size.height;
    slider.frame = NSMakeRect(from, floor((tall - h) / 2.0), TRAY_WIDE - from - TRAY_SIDE, h);
    [row addSubview:slider];
    return tray_item_with(row);
}

static void tray_dress_sleep(void) {
    if (tray_sleep_key == nil) {
        return;
    }
    CGFloat wide = MIN(tray_key_wide(tray_sleep_key, tray_label_sleep), TRAY_WIDE - 2.0 * (TRAY_SIDE - TRAY_KEY_PAD));
    NSRect frame = tray_sleep_key.frame;
    frame.size.width = wide;
    tray_sleep_key.frame = frame;
    tray_sleep_item.hidden = tray_label_sleep.length == 0;
}

static NSMenuItem *tray_sleep_row(void) {
    const CGFloat tall = 26.0;
    NSView *row = [[NSView alloc] initWithFrame:NSMakeRect(0, 0, TRAY_WIDE, tall)];
    NSButton *key = tray_key(tray_label_sleep, @"", @selector(sleepStep:));
    CGFloat h = key.frame.size.height;
    key.frame = NSMakeRect(TRAY_SIDE - TRAY_KEY_PAD, floor((tall - h) / 2.0), 0.0, h);
    [row addSubview:key];
    tray_sleep_key = key;
    tray_sleep_item = tray_item_with(row);
    tray_dress_sleep();
    return tray_sleep_item;
}

int kessler_tray_cover_points(void) {
    NSTextField *probe = [NSTextField labelWithString:@"Ag"];
    probe.font = [NSFont menuFontOfSize:0];
    NSSize fits = [probe sizeThatFits:NSMakeSize(CGFLOAT_MAX, CGFLOAT_MAX)];
    return (int)(3.0 * ceil(fits.height) + 2.0);
}

static NSMenuItem *tray_about_row(void) {
    const CGFloat side = TRAY_SIDE;
    const CGFloat below = 8.0;
    const CGFloat between = 1.0;
    const CGFloat apart = 10.0;
    const CGFloat full = TRAY_WIDE - 2.0 * side;
    NSImage *cover = tray_cover_at == 0 ? nil : tray_cover;
    BOOL beside = cover != nil && tray_cover_at == 1;
    const CGFloat above = cover != nil && !beside ? side - TRAY_MENU_TOP : 8.0;
    CGFloat picture = cover == nil ? 0.0 : (beside ? cover.size.height : full);
    CGFloat room = beside ? full - picture - apart : full;
    NSMutableArray<NSTextField *> *fields = [NSMutableArray array];
    NSMutableArray<NSNumber *> *heights = [NSMutableArray array];
    for (NSString *line in [tray_title componentsSeparatedByString:@"\n"]) {
        if (line.length == 0) {
            continue;
        }
        NSTextField *field = [NSTextField wrappingLabelWithString:line];
        field.selectable = NO;
        field.font = [NSFont menuFontOfSize:0];
        field.textColor = fields.count == 0 ? [NSColor labelColor] : [NSColor secondaryLabelColor];
        field.preferredMaxLayoutWidth = room;
        NSSize fits = [field sizeThatFits:NSMakeSize(room, CGFLOAT_MAX)];
        [fields addObject:field];
        [heights addObject:@(ceil(fits.height))];
    }
    CGFloat words = 0.0;
    for (NSUInteger i = 0; i < heights.count; i++) {
        words += heights[i].doubleValue + (i > 0 ? between : 0.0);
    }
    CGFloat body = beside ? MAX(words, picture) : words + (cover != nil ? picture + apart : 0.0);
    CGFloat tall = above + body + below;
    NSView *box = [[NSView alloc] initWithFrame:NSMakeRect(0, 0, TRAY_WIDE, tall)];
    CGFloat top = tall - above;
    CGFloat left = side;
    if (cover != nil) {
        NSImageView *view = [NSImageView imageViewWithImage:cover];
        view.imageScaling = NSImageScaleProportionallyUpOrDown;
        view.frame = NSMakeRect(side, top - picture, picture, picture);
        [box addSubview:view];
        if (beside) {
            left = side + picture + apart;
            top -= floor((body - words) / 2.0);
        } else {
            top -= picture + apart;
        }
    }
    for (NSUInteger i = 0; i < fields.count; i++) {
        CGFloat h = heights[i].doubleValue;
        top -= h;
        fields[i].frame = NSMakeRect(left, top, room, h);
        [box addSubview:fields[i]];
        top -= between;
    }
    return tray_item_with(box);
}

static NSMenu *tray_menu(void) {
    NSMenu *menu = [[NSMenu alloc] initWithTitle:@"kessler"];
    menu.delegate = tray_target;
    tray_about_item = nil;
    if (tray_title.length > 0) {
        tray_about_item = tray_about_row();
        [menu addItem:tray_about_item];
    }
    if (tray_about_item != nil) {
        [menu addItem:[NSMenuItem separatorItem]];
    }
    [menu addItem:tray_controls()];
    [menu addItem:tray_sleep_row()];
    [menu addItem:[NSMenuItem separatorItem]];
    [menu addItem:tray_entry(tray_label_show, @selector(show:))];
    [menu addItem:tray_entry(tray_label_quit, @selector(quit:))];
    return menu;
}

static void tray_rebuild_about(void) {
    if (tray_open_menu == nil) {
        return;
    }
    NSInteger at = tray_about_item == nil ? -1 : [tray_open_menu indexOfItem:tray_about_item];
    if (at >= 0) {
        [tray_open_menu removeItemAtIndex:at];
        [tray_open_menu removeItemAtIndex:at];
    }
    tray_about_item = nil;
    if (tray_title.length > 0) {
        tray_about_item = tray_about_row();
        [tray_open_menu insertItem:tray_about_item atIndex:0];
        [tray_open_menu insertItem:[NSMenuItem separatorItem] atIndex:1];
    }
}

@implementation KesslerTray
- (void)clicked:(id)sender {
    (void)sender;
    NSEvent *event = [NSApp currentEvent];
    BOOL wants_menu = event.type == NSEventTypeRightMouseUp ||
                      (event.modifierFlags & NSEventModifierFlagControl) != 0;
    if (!wants_menu) {
        tray_playing = !tray_playing;
        tray_push(KESSLER_TRAY_TOGGLE);
        return;
    }
    tray_open_menu = tray_menu();
    tray_item.menu = tray_open_menu;
    [tray_item.button performClick:nil];
    tray_item.menu = nil;
}
- (void)menuDidClose:(NSMenu *)menu {
    (void)menu;
    tray_play_key = nil;
    tray_open_menu = nil;
    tray_about_item = nil;
    tray_sleep_item = nil;
    tray_sleep_key = nil;
}
- (void)toggle:(id)sender {
    if (sender != nil && sender == tray_play_key) {
        tray_playing = !tray_playing;
        tray_dress_play(tray_play_key);
    }
    tray_push(KESSLER_TRAY_TOGGLE);
}
- (void)next:(id)sender {
    (void)sender;
    tray_push(KESSLER_TRAY_NEXT);
}
- (void)previous:(id)sender {
    (void)sender;
    tray_push(KESSLER_TRAY_PREVIOUS);
}
- (void)sleepStep:(id)sender {
    (void)sender;
    tray_push(KESSLER_TRAY_SLEEP);
}
- (void)volume:(id)sender {
    int percent = (int)([(NSSlider *)sender doubleValue] + 0.5);
    percent = percent < 0 ? 0 : (percent > 100 ? 100 : percent);
    if (tray_lock != nil) {
        [tray_lock lock];
        tray_volume_set = percent;
        tray_volume = percent;
        [tray_lock unlock];
    }
    kessler_media_woken();
}
- (void)show:(id)sender {
    (void)sender;
    [NSApp unhide:nil];
    [NSApp activateIgnoringOtherApps:YES];
    for (NSWindow *window in [NSApp windows]) {
        if ([window isMiniaturized]) {
            [window deminiaturize:nil];
        }
        if ([window canBecomeMainWindow]) {
            [window makeKeyAndOrderFront:nil];
        }
    }
    kessler_media_woken();
}
- (void)quit:(id)sender {
    (void)sender;
    [NSApp terminate:nil];
}
@end

void kessler_tray_show(int show) {
    if (tray_lock == nil) {
        tray_lock = [[NSLock alloc] init];
        tray_target = [[KesslerTray alloc] init];
    }
    if (show && tray_item == nil) {
        tray_item = [[NSStatusBar systemStatusBar] statusItemWithLength:NSVariableStatusItemLength];
        tray_item.button.target = tray_target;
        tray_item.button.action = @selector(clicked:);
        [tray_item.button sendActionOn:(NSEventMaskLeftMouseUp | NSEventMaskRightMouseUp)];
        tray_item.button.toolTip = @"kessler";
    } else if (!show && tray_item != nil) {
        [[NSStatusBar systemStatusBar] removeStatusItem:tray_item];
        tray_item = nil;
    }
}

void kessler_tray_image(const unsigned char *rgba, int wide, int tall) {
    if (rgba == NULL || wide <= 0 || tall <= 0) {
        return;
    }
    NSData *pixels = [NSData dataWithBytes:rgba length:(NSUInteger)wide * (NSUInteger)tall * 4];
    dispatch_async(dispatch_get_main_queue(), ^{
        if (tray_item == nil) {
            return;
        }
        NSBitmapImageRep *rep = [[NSBitmapImageRep alloc]
            initWithBitmapDataPlanes:NULL
                          pixelsWide:wide
                          pixelsHigh:tall
                       bitsPerSample:8
                     samplesPerPixel:4
                            hasAlpha:YES
                            isPlanar:NO
                      colorSpaceName:NSDeviceRGBColorSpace
                         bytesPerRow:wide * 4
                        bitsPerPixel:32];
        if (rep == nil) {
            return;
        }
        memcpy([rep bitmapData], pixels.bytes, pixels.length);
        NSImage *image = [[NSImage alloc] initWithSize:NSMakeSize(wide / 2.0, tall / 2.0)];
        [image addRepresentation:rep];
        image.template = YES;
        tray_item.button.image = image;
    });
}

static void tray_refresh_open_menu(NSString *before, BOOL was_playing, NSString *sleep_before);

void kessler_tray_state(int playing,
                        const char *title,
                        const char *play,
                        const char *pause,
                        const char *next,
                        const char *previous,
                        const char *show,
                        const char *quit,
                        int volume,
                        const char *volume_word,
                        const char *sleep) {
    NSString *before = tray_title;
    NSString *sleep_before = tray_label_sleep;
    BOOL was_playing = tray_playing;
    tray_playing = playing ? YES : NO;
    tray_title = tray_string(title);
    tray_label_play = tray_string(play);
    tray_label_pause = tray_string(pause);
    tray_label_next = tray_string(next);
    tray_label_previous = tray_string(previous);
    tray_label_show = tray_string(show);
    tray_label_quit = tray_string(quit);
    tray_label_volume = tray_string(volume_word);
    tray_label_sleep = tray_string(sleep);
    tray_volume = volume < 0 ? 0 : (volume > 100 ? 100 : volume);
    tray_refresh_open_menu(before, was_playing, sleep_before);
}

static void tray_refresh_open_menu(NSString *before, BOOL was_playing, NSString *sleep_before) {
    void (^refresh)(void) = ^{
        if (tray_open_menu == nil) {
            return;
        }
        if (![before isEqualToString:tray_title]) {
            tray_rebuild_about();
        }
        if (was_playing != tray_playing && tray_play_key != nil) {
            tray_dress_play(tray_play_key);
        }
        if (![sleep_before isEqualToString:tray_label_sleep]) {
            tray_dress_sleep();
        }
    };
    if ([NSThread isMainThread]) {
        refresh();
    } else {
        dispatch_async(dispatch_get_main_queue(), refresh);
    }
}

void kessler_tray_cover(const unsigned char *rgba, int side, int at) {
    NSData *pixels = nil;
    if (rgba != NULL && side > 0) {
        pixels = [NSData dataWithBytes:rgba length:(NSUInteger)side * (NSUInteger)side * 4];
    }
    dispatch_async(dispatch_get_main_queue(), ^{
        NSImage *image = nil;
        if (pixels != nil) {
            NSBitmapImageRep *rep = [[NSBitmapImageRep alloc]
                initWithBitmapDataPlanes:NULL
                              pixelsWide:side
                              pixelsHigh:side
                           bitsPerSample:8
                         samplesPerPixel:4
                                hasAlpha:YES
                                isPlanar:NO
                          colorSpaceName:NSDeviceRGBColorSpace
                             bytesPerRow:side * 4
                            bitsPerPixel:32];
            if (rep != nil) {
                memcpy([rep bitmapData], pixels.bytes, pixels.length);
                image = [[NSImage alloc] initWithSize:NSMakeSize(side / 2.0, side / 2.0)];
                [image addRepresentation:rep];
            }
        }
        tray_cover = image;
        tray_cover_at = image == nil ? 0 : at;
        tray_rebuild_about();
    });
}

int kessler_tray_volume(void) {
    if (tray_lock == nil) {
        return -1;
    }
    [tray_lock lock];
    int percent = tray_volume_set;
    tray_volume_set = -1;
    [tray_lock unlock];
    return percent;
}

int kessler_tray_poll(void) {
    if (tray_lock == nil) {
        return KESSLER_TRAY_NONE;
    }
    [tray_lock lock];
    int command = KESSLER_TRAY_NONE;
    if (tray_head != tray_tail) {
        command = tray_queue[tray_head];
        tray_head = (tray_head + 1) % KESSLER_TRAY_QUEUE;
    }
    [tray_lock unlock];
    return command;
}
