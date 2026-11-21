#import <AppKit/AppKit.h>
#import <Foundation/Foundation.h>

enum {
    KESSLER_TB_NONE = 0,
    KESSLER_TB_TOGGLE = 3,
    KESSLER_TB_NEXT = 4,
    KESSLER_TB_PREVIOUS = 5,
};

#define KESSLER_TB_QUEUE 16

static int kessler_tb_queue[KESSLER_TB_QUEUE];
static int kessler_tb_head = 0;
static int kessler_tb_tail = 0;
static NSLock *kessler_tb_lock = nil;

extern void kessler_media_woken(void);

static void kessler_tb_push(int command) {
    if (kessler_tb_lock == nil) {
        return;
    }
    [kessler_tb_lock lock];
    int next = (kessler_tb_tail + 1) % KESSLER_TB_QUEUE;
    if (next != kessler_tb_head) {
        kessler_tb_queue[kessler_tb_tail] = command;
        kessler_tb_tail = next;
    }
    [kessler_tb_lock unlock];
    kessler_media_woken();
}

int kessler_touchbar_poll(void) {
    if (kessler_tb_lock == nil) {
        return KESSLER_TB_NONE;
    }
    [kessler_tb_lock lock];
    int command = KESSLER_TB_NONE;
    if (kessler_tb_head != kessler_tb_tail) {
        command = kessler_tb_queue[kessler_tb_head];
        kessler_tb_head = (kessler_tb_head + 1) % KESSLER_TB_QUEUE;
    }
    [kessler_tb_lock unlock];
    return command;
}

static int kessler_tb_state = 0;
static NSString *kessler_tb_title = nil;
static NSString *kessler_tb_artist = nil;
static NSString *kessler_tb_artwork_path = nil;
static NSImage *kessler_tb_artwork = nil;
static BOOL kessler_tb_lower = YES;

static NSString *kessler_tb_string(const char *text) {
    if (text == NULL) {
        return @"";
    }
    NSString *string = [NSString stringWithUTF8String:text];
    return string == nil ? @"" : string;
}

static NSTouchBarItemIdentifier const KesslerTBCover = @"de.windparksoft.kessler.touchbar.cover";
static NSTouchBarItemIdentifier const KesslerTBTitle = @"de.windparksoft.kessler.touchbar.title";
static NSTouchBarItemIdentifier const KesslerTBPicture = @"de.windparksoft.kessler.touchbar.picture";
static NSTouchBarItemIdentifier const KesslerTBPrevious = @"de.windparksoft.kessler.touchbar.previous";
static NSTouchBarItemIdentifier const KesslerTBToggle = @"de.windparksoft.kessler.touchbar.toggle";
static NSTouchBarItemIdentifier const KesslerTBNext = @"de.windparksoft.kessler.touchbar.next";

static const CGFloat KesslerTBTitleWidest = 180.0;

static const CGFloat KesslerTBPictureLeast = 120.0;
static const CGFloat KesslerTBPictureMost = 1000.0;
static const CGFloat KesslerTBPictureTall = 30.0;

static BOOL kessler_tb_whole = NO;
static BOOL kessler_tb_station = NO;

static NSImage *kessler_tb_picture = nil;

API_AVAILABLE(macos(10.12.2))
@interface KesslerTouchBar : NSObject <NSTouchBarDelegate>
@property(strong) NSTouchBar *bar;
@property(strong) NSImageView *cover;
@property(strong) NSTextField *title;
@property(strong) NSImageView *picture;
@property(strong) NSButton *toggle;
- (void)refresh;
- (void)arrange;
@end

@implementation KesslerTouchBar

- (instancetype)init {
    self = [super init];
    if (self == nil) {
        return nil;
    }
    _bar = [[NSTouchBar alloc] init];
    _bar.delegate = self;
    [self arrange];
    return self;
}

- (void)arrange {
    if (kessler_tb_whole) {
        self.bar.defaultItemIdentifiers = @[ KesslerTBCover, KesslerTBPicture ];
    } else if (kessler_tb_station) {
        self.bar.defaultItemIdentifiers = @[
            KesslerTBCover, KesslerTBTitle, NSTouchBarItemIdentifierFixedSpaceSmall, KesslerTBPicture,
            NSTouchBarItemIdentifierFixedSpaceSmall, KesslerTBToggle, KesslerTBNext,
        ];
    } else {
        self.bar.defaultItemIdentifiers = @[
            KesslerTBCover, KesslerTBTitle, NSTouchBarItemIdentifierFixedSpaceSmall, KesslerTBPicture,
            NSTouchBarItemIdentifierFixedSpaceSmall, KesslerTBPrevious, KesslerTBToggle, KesslerTBNext,
        ];
    }
}

- (void)previous:(id)sender {
    (void)sender;
    kessler_tb_push(KESSLER_TB_PREVIOUS);
}

- (void)togglePlay:(id)sender {
    (void)sender;
    kessler_tb_push(KESSLER_TB_TOGGLE);
}

- (void)next:(id)sender {
    (void)sender;
    kessler_tb_push(KESSLER_TB_NEXT);
}

- (NSCustomTouchBarItem *)button:(NSTouchBarItemIdentifier)identifier
                           image:(NSImageName)name
                          action:(SEL)action {
    NSCustomTouchBarItem *item = [[NSCustomTouchBarItem alloc] initWithIdentifier:identifier];
    NSImage *image = [NSImage imageNamed:name];
    NSButton *button = [NSButton buttonWithImage:(image != nil ? image : [[NSImage alloc] init])
                                          target:self
                                          action:action];
    item.view = button;
    item.visibilityPriority = NSTouchBarItemPriorityHigh;
    if ([identifier isEqualToString:KesslerTBToggle]) {
        self.toggle = button;
    }
    return item;
}

- (NSTouchBarItem *)touchBar:(NSTouchBar *)touchBar
       makeItemForIdentifier:(NSTouchBarItemIdentifier)identifier {
    (void)touchBar;
    if ([identifier isEqualToString:KesslerTBCover]) {
        NSCustomTouchBarItem *item = [[NSCustomTouchBarItem alloc] initWithIdentifier:identifier];
        NSImageView *view = [[NSImageView alloc] initWithFrame:NSMakeRect(0, 0, 30, 30)];
        view.imageScaling = NSImageScaleProportionallyUpOrDown;
        [view.widthAnchor constraintEqualToConstant:30].active = YES;
        [view.heightAnchor constraintEqualToConstant:30].active = YES;
        self.cover = view;
        item.view = view;
        item.visibilityPriority = NSTouchBarItemPriorityHigh;
        [self refresh];
        return item;
    }
    if ([identifier isEqualToString:KesslerTBTitle]) {
        NSCustomTouchBarItem *item = [[NSCustomTouchBarItem alloc] initWithIdentifier:identifier];
        NSTextField *label = [NSTextField labelWithString:@""];
        label.lineBreakMode = NSLineBreakByTruncatingTail;
        [label.widthAnchor constraintLessThanOrEqualToConstant:KesslerTBTitleWidest].active = YES;
        [label setContentCompressionResistancePriority:NSLayoutPriorityDefaultLow
                                        forOrientation:NSLayoutConstraintOrientationHorizontal];
        self.title = label;
        item.view = label;
        item.visibilityPriority = NSTouchBarItemPriorityNormal;
        [self refresh];
        return item;
    }
    if ([identifier isEqualToString:KesslerTBPicture]) {
        NSCustomTouchBarItem *item = [[NSCustomTouchBarItem alloc] initWithIdentifier:identifier];
        NSImageView *view = [[NSImageView alloc] initWithFrame:NSMakeRect(0, 0, KesslerTBPictureLeast, KesslerTBPictureTall)];
        view.imageScaling = NSImageScaleNone;
        [view.widthAnchor constraintGreaterThanOrEqualToConstant:KesslerTBPictureLeast].active = YES;
        [view.widthAnchor constraintLessThanOrEqualToConstant:KesslerTBPictureMost].active = YES;
        NSLayoutConstraint *wide = [view.widthAnchor constraintEqualToConstant:KesslerTBPictureMost];
        wide.priority = NSLayoutPriorityDefaultLow - 1;
        wide.active = YES;
        [view setContentHuggingPriority:NSLayoutPriorityDefaultLow - 2
                         forOrientation:NSLayoutConstraintOrientationHorizontal];
        [view setContentCompressionResistancePriority:NSLayoutPriorityDefaultLow - 2
                                       forOrientation:NSLayoutConstraintOrientationHorizontal];
        [view.heightAnchor constraintEqualToConstant:KesslerTBPictureTall].active = YES;
        if (kessler_tb_whole) {
            NSClickGestureRecognizer *touch = [[NSClickGestureRecognizer alloc] initWithTarget:self
                                                                                        action:@selector(togglePlay:)];
            touch.allowedTouchTypes = NSTouchTypeMaskDirect;
            [view addGestureRecognizer:touch];
        }
        self.picture = view;
        item.view = view;
        item.visibilityPriority = kessler_tb_whole ? NSTouchBarItemPriorityHigh : NSTouchBarItemPriorityLow;
        [self refresh];
        return item;
    }
    if ([identifier isEqualToString:KesslerTBPrevious]) {
        return [self button:identifier image:NSImageNameTouchBarSkipBackTemplate action:@selector(previous:)];
    }
    if ([identifier isEqualToString:KesslerTBToggle]) {
        NSCustomTouchBarItem *item = [self button:identifier
                                            image:NSImageNameTouchBarPlayTemplate
                                           action:@selector(togglePlay:)];
        [self refresh];
        return item;
    }
    if ([identifier isEqualToString:KesslerTBNext]) {
        return [self button:identifier image:NSImageNameTouchBarSkipAheadTemplate action:@selector(next:)];
    }
    return nil;
}

- (void)refresh {
    if (self.cover != nil) {
        self.cover.image = kessler_tb_artwork;
        self.cover.hidden = (kessler_tb_artwork == nil);
    }
    if (self.picture != nil) {
        self.picture.image = kessler_tb_picture;
        self.picture.hidden = (kessler_tb_picture == nil);
    }
    if (self.title != nil) {
        NSString *title = kessler_tb_title ?: @"";
        NSString *artist = kessler_tb_artist ?: @"";
        NSString *said = artist.length > 0 && title.length > 0
            ? [NSString stringWithFormat:@"%@ · %@", title, artist]
            : (title.length > 0 ? title : @"kessler");
        self.title.stringValue = kessler_tb_lower ? [said lowercaseString] : said;
    }
    if (self.toggle != nil) {
        NSImage *image = [NSImage imageNamed:(kessler_tb_state == 1 ? NSImageNameTouchBarPauseTemplate
                                                                    : NSImageNameTouchBarPlayTemplate)];
        if (image != nil) {
            self.toggle.image = image;
        }
    }
}

@end

static id kessler_tb_strip = nil;

void kessler_touchbar_update(int state, const char *title, const char *artist, const char *artwork) {
    kessler_tb_state = state;
    kessler_tb_title = state == 0 ? @"" : kessler_tb_string(title);
    kessler_tb_artist = state == 0 ? @"" : kessler_tb_string(artist);
    NSString *path = state == 0 ? @"" : kessler_tb_string(artwork);
    if (kessler_tb_artwork_path == nil || ![kessler_tb_artwork_path isEqualToString:path]) {
        kessler_tb_artwork_path = [path copy];
        kessler_tb_artwork = path.length > 0 ? [[NSImage alloc] initWithContentsOfFile:path] : nil;
    }
    if (@available(macOS 10.12.2, *)) {
        if (kessler_tb_strip != nil) {
            [(KesslerTouchBar *)kessler_tb_strip refresh];
        }
    }
}

int kessler_touchbar_picture_wide(void) {
    if (@available(macOS 10.12.2, *)) {
        KesslerTouchBar *strip = (KesslerTouchBar *)kessler_tb_strip;
        if (strip != nil && strip.picture != nil) {
            return (int)strip.picture.frame.size.width;
        }
    }
    return 0;
}

void kessler_touchbar_show(int show, int lower, int whole, int station) {
    kessler_tb_lower = lower != 0;
    BOOL was_whole = kessler_tb_whole;
    kessler_tb_whole = whole != 0;
    kessler_tb_station = station != 0;
    if (kessler_tb_lock == nil) {
        kessler_tb_lock = [[NSLock alloc] init];
    }
    if (@available(macOS 10.12.2, *)) {
        if (show) {
            if (kessler_tb_strip == nil || was_whole != kessler_tb_whole) {
                kessler_tb_strip = [[KesslerTouchBar alloc] init];
            }
            [(KesslerTouchBar *)kessler_tb_strip arrange];
            NSApp.touchBar = ((KesslerTouchBar *)kessler_tb_strip).bar;
            [(KesslerTouchBar *)kessler_tb_strip refresh];
        } else {
            NSApp.touchBar = nil;
            kessler_tb_strip = nil;
        }
    }
}

void kessler_touchbar_picture(const unsigned char *rgba, int wide, int tall) {
    NSImage *image = nil;
    if (rgba != NULL && wide > 0 && tall > 0) {
        NSBitmapImageRep *rep = [[NSBitmapImageRep alloc] initWithBitmapDataPlanes:NULL
                                                                        pixelsWide:wide
                                                                        pixelsHigh:tall
                                                                     bitsPerSample:8
                                                                   samplesPerPixel:4
                                                                          hasAlpha:YES
                                                                          isPlanar:NO
                                                                    colorSpaceName:NSDeviceRGBColorSpace
                                                                      bitmapFormat:NSBitmapFormatAlphaNonpremultiplied
                                                                       bytesPerRow:(NSInteger)wide * 4
                                                                      bitsPerPixel:32];
        if (rep != nil && rep.bitmapData != NULL) {
            memcpy(rep.bitmapData, rgba, (size_t)wide * (size_t)tall * 4);
            rep.size = NSMakeSize(wide / 2.0, tall / 2.0);
            image = [[NSImage alloc] initWithSize:rep.size];
            [image addRepresentation:rep];
        }
    }
    kessler_tb_picture = image;
    if (@available(macOS 10.12.2, *)) {
        if (kessler_tb_strip != nil) {
            [(KesslerTouchBar *)kessler_tb_strip refresh];
        }
    }
}
