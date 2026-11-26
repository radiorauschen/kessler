//! The window's three buttons on a Mac, set in from the corner.
//!
//! A Mac sets them further in when the window has a toolbar. But a toolbar
//! takes every click in its band, and every view sets a line of its own in
//! that band. So the buttons are always set by hand, where
//! the taller toolbar put them, on a band with no toolbar — the way Electron's
//! `trafficLightPosition` does it — and the window is dragged by the band's
//! empty parts (`library::render`). AppKit itself would move the window from
//! anywhere in that band, the timeline and the volume included, so the view
//! says it cannot, see `held_still`.

/// Where the middle of the buttons stands, from the window's top edge: where
/// the taller toolbar put it.
pub const BY_HAND_MIDDLE: f32 = 28.0;

/// And how far the first stands in from the window's left edge.
#[cfg(target_os = "macos")]
const BY_HAND_LEFT: f64 = 20.0;

/// What setting the buttons came to: how tall the band at the top is, where
/// the last button ends, from the window's left edge, and where the middle of
/// each stands and how wide a circle they are — for kessler's own circles in
/// their place.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placed {
    pub band: f32,
    pub buttons_end: f32,
    pub middles: [f32; 3],
    pub across: f32,
}

/// Whether kessler draws the three buttons itself (`[ui] window_buttons`):
/// the Mac's own are then hidden, in their place and still laid out, so the
/// band and everything beside it stay where they are.
static OWN_BUTTONS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Say whether kessler draws the buttons itself, each frame before `apply`.
pub fn own_buttons(own: bool) {
    OWN_BUTTONS.store(own, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(target_os = "macos")]
mod by_hand {
    use objc2_app_kit::NSWindow;
    use objc2_foundation::NSPoint;

    fn buttons(window: &NSWindow) -> Option<[objc2::rc::Retained<objc2_app_kit::NSButton>; 3]> {
        use objc2_app_kit::NSWindowButton;
        Some([
            window.standardWindowButton(NSWindowButton::CloseButton)?,
            window.standardWindowButton(NSWindowButton::MiniaturizeButton)?,
            window.standardWindowButton(NSWindowButton::ZoomButton)?,
        ])
    }

    /// The view that holds the band: the buttons' superview's superview, as
    /// AppKit builds it.
    fn band_view(button: &objc2_app_kit::NSButton) -> Option<objc2::rc::Retained<objc2_app_kit::NSView>> {
        unsafe { button.superview()?.superview() }
    }

    /// Set them where the taller toolbar would, and say where they end, where
    /// each one's middle is and how wide they are.
    pub fn set(window: &NSWindow) -> Option<(f32, [f32; 3], f32)> {
        use objc2_app_kit::NSWindowStyleMask;
        let [close, shrink, zoom] = buttons(window)?;
        let full = window.styleMask().contains(NSWindowStyleMask::FullScreen);
        let hidden = !full && super::OWN_BUTTONS.load(std::sync::atomic::Ordering::Relaxed);
        for button in [&close, &shrink, &zoom] {
            if button.isHidden() != hidden {
                button.setHidden(hidden);
            }
        }
        if full {
            return None;
        }
        let band = band_view(&close)?;
        let height = f64::from(super::BY_HAND_MIDDLE) * 2.0;
        let mut frame = band.frame();
        frame.size.height = height;
        frame.origin.y = window.frame().size.height - height;
        if band.frame() != frame {
            band.setFrame(frame);
        }
        let step = STEP.get().unwrap_or_else(|| {
            let measured = shrink.frame().origin.x - close.frame().origin.x;
            let step = if (14.0..=32.0).contains(&measured) { measured } else { STANDARD_STEP };
            STEP.set(Some(step));
            step
        });
        let mut end = 0.0;
        let mut middles = [0.0f32; 3];
        let mut across = 0.0f64;
        for (at, button) in [&close, &shrink, &zoom].into_iter().enumerate() {
            let size = button.frame().size;
            let origin = NSPoint::new(super::BY_HAND_LEFT + step * at as f64, (height - size.height) / 2.0);
            if button.frame().origin != origin {
                button.setFrameOrigin(origin);
            }
            end = origin.x + size.width;
            middles[at] = (origin.x + size.width / 2.0) as f32;
            across = size.width.min(size.height) - 2.0;
        }
        Some((end as f32, middles, across.max(8.0) as f32))
    }

    thread_local! {
        /// The views and the window `keep` watches, so a button or band
        /// AppKit makes anew (the green one as the window zooms, say, or
        /// all of them after full screen) is watched as well.
        static WATCHED: std::cell::RefCell<Vec<*const objc2::runtime::AnyObject>> =
            const { std::cell::RefCell::new(Vec::new()) };
        /// Whether `set` is running already: its own moves call it again.
        static SETTING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
        /// Whether AppKit moved something while `set` ran, so it runs again.
        static AGAIN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
        /// The gap between the buttons, as `set` first found it.
        static STEP: std::cell::Cell<Option<f64>> = const { std::cell::Cell::new(None) };
    }

    /// The gap between the buttons' left edges on the Macs of these years,
    /// should the measured one make no sense.
    const STANDARD_STEP: f64 = 20.0;

    /// How often `keep` runs `set` again for moves AppKit made meanwhile.
    const AT_MOST: usize = 8;

    /// Put the buttons back the moment AppKit moves them, not a frame later.
    /// While the window zooms (a double click on the band) AppKit animates it
    /// in a loop of its own, lays the band out at every step and kessler draws
    /// no frame: the buttons stood in the corner until it was done. So whenever
    /// a button or the band changes its frame, `set` runs at once, on the main
    /// thread, inside AppKit's layout — and whenever the window has changed its
    /// size: the green button still stood where AppKit wanted it all through
    /// the zoom, so AppKit moves it without telling, or makes it anew.
    pub fn keep(window: &NSWindow) {
        use objc2_app_kit::{NSViewFrameDidChangeNotification, NSWindowDidResizeNotification};
        use objc2_foundation::NSNotificationCenter;
        let Some(buttons) = buttons(window) else {
            return;
        };
        let Some(band) = band_view(&buttons[0]) else {
            return;
        };
        let [close, shrink, zoom] = buttons;
        let views = [&*close, &*shrink, &*zoom, &*band].map(|view| -> &objc2::runtime::AnyObject { view });
        let window_object: &objc2::runtime::AnyObject = window;
        let new: Vec<_> = WATCHED.with_borrow(|watched| {
            views
                .into_iter()
                .chain([window_object])
                .filter(|object| !watched.contains(&(*object as *const _)))
                .collect()
        });
        if new.is_empty() {
            return;
        }
        let weak = objc2::rc::Weak::new(window);
        let block = block2::RcBlock::new(move |_| {
            if SETTING.replace(true) {
                AGAIN.set(true);
                return;
            }
            if let Some(window) = weak.load() {
                keep(&window);
                for _ in 0..AT_MOST {
                    AGAIN.set(false);
                    let _ = set(&window);
                    if !AGAIN.get() {
                        break;
                    }
                }
            }
            SETTING.set(false);
        });
        let centre = NSNotificationCenter::defaultCenter();
        for object in new {
            WATCHED.with_borrow_mut(|watched| watched.push(object as *const _));
            let name = if std::ptr::eq(object, window_object) {
                unsafe { NSWindowDidResizeNotification }
            } else {
                let view: &objc2_app_kit::NSView = unsafe { &*(object as *const _ as *const objc2_app_kit::NSView) };
                view.setPostsFrameChangedNotifications(true);
                unsafe { NSViewFrameDidChangeNotification }
            };
            let token = unsafe { centre.addObserverForName_object_queue_usingBlock(Some(name), Some(object), None, &block) };
            std::mem::forget(token);
        }
    }
}

/// Whether AppKit may move the window from a press on kessler's view: in the
/// band at the top of a window whose ground runs up behind its title bar, it
/// asks the view under the pointer, and a view that says nothing says yes.
/// The band is the masthead's, so the view says no and kessler moves the
/// window itself, from the band's empty parts only.
#[cfg(target_os = "macos")]
mod held_still {
    use objc2::runtime::{AnyObject, Bool, Sel};
    use std::sync::Once;

    static TAUGHT: Once = Once::new();

    extern "C-unwind" fn may_move(_view: &AnyObject, _cmd: Sel) -> Bool {
        Bool::NO
    }

    pub fn set(view: &objc2_app_kit::NSView) {
        TAUGHT.call_once(|| {
            use objc2::Encode;
            let types = std::ffi::CString::new(format!("{}@:", Bool::ENCODING)).expect("no nul");
            unsafe {
                let class = (view as &AnyObject).class() as *const _ as *mut objc2::runtime::AnyClass;
                let imp: objc2::runtime::Imp = std::mem::transmute(
                    may_move as extern "C-unwind" fn(&AnyObject, Sel) -> Bool,
                );
                objc2::ffi::class_addMethod(class, objc2::sel!(mouseDownCanMoveWindow), imp, types.as_ptr());
            }
        });
    }
}

/// A window's frame, in the screen's points, from its lower left corner.
#[cfg(any(target_os = "macos", test))]
#[derive(Debug, Clone, Copy, PartialEq)]
struct Frame {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

#[cfg(any(target_os = "macos", test))]
impl Frame {
    /// Whether it fills `screen`, give or take a point or two.
    fn fills(self, screen: Frame) -> bool {
        (self.x - screen.x).abs() < 2.0
            && (self.y - screen.y).abs() < 2.0
            && (self.width - screen.width).abs() < 2.0
            && (self.height - screen.height).abs() < 2.0
    }

    /// Where a window that fills the screen goes back to when nothing was
    /// remembered: two thirds of the screen, in its middle.
    fn middle_of(screen: Frame) -> Frame {
        let (width, height) = ((screen.width * 2.0 / 3.0).round(), (screen.height * 2.0 / 3.0).round());
        Frame {
            x: (screen.x + (screen.width - width) / 2.0).round(),
            y: (screen.y + (screen.height - height) / 2.0).round(),
            width,
            height,
        }
    }
}

/// Where a zoom of `window` on `screen` goes, and what to remember: to the
/// screen, remembering where it was; or, filling it already, back to what
/// was remembered.
#[cfg(any(target_os = "macos", test))]
fn zoom_target(window: Frame, screen: Frame, remembered: Option<Frame>) -> (Frame, Option<Frame>) {
    if window.fills(screen) {
        (remembered.unwrap_or_else(|| Frame::middle_of(screen)), None)
    } else {
        (screen, Some(window))
    }
}

/// The band was double-clicked: grow the window to the screen, or back.
/// Done at the start of the next frame.
pub fn zoom() {
    #[cfg(target_os = "macos")]
    zooming::ASKED.with(|asked| asked.set(true));
}

/// Lay the window in the Dock, as the Mac's own yellow button does — asked by
/// kessler's own circle in its place, done at the start of the next frame.
///
/// Not through egui's `Minimized`: on a Mac egui notes the window as
/// minimized when told to and never looks again, so once it came back out of
/// the Dock it was still taken for hidden, and nothing was drawn any more —
/// the last picture shrank with the window and nothing answered.
pub fn shrink_into_dock() {
    #[cfg(target_os = "macos")]
    zooming::SHRINK.with(|asked| asked.set(true));
}

#[cfg(target_os = "macos")]
mod zooming {
    use super::{zoom_target, Frame};
    use objc2_app_kit::NSWindow;
    use objc2_foundation::{NSPoint, NSRect, NSSize};
    use std::cell::Cell;

    thread_local! {
        /// Whether a zoom was asked for.
        pub static ASKED: Cell<bool> = const { Cell::new(false) };
        /// Whether the window is to go into the Dock.
        pub static SHRINK: Cell<bool> = const { Cell::new(false) };
        /// Where the window stood before it was made as big as the screen.
        static REMEMBERED: Cell<Option<Frame>> = const { Cell::new(None) };
    }

    fn frame(rect: NSRect) -> Frame {
        Frame { x: rect.origin.x, y: rect.origin.y, width: rect.size.width, height: rect.size.height }
    }

    fn rect(frame: Frame) -> NSRect {
        NSRect::new(NSPoint::new(frame.x, frame.y), NSSize::new(frame.width, frame.height))
    }

    /// Do a zoom that was asked for: straight there, and the frame being
    /// drawn shows it.
    pub fn step(window: &NSWindow) {
        if SHRINK.with(|asked| asked.replace(false)) {
            window.miniaturize(None);
        }
        if !ASKED.with(|asked| asked.replace(false)) {
            return;
        }
        if window.styleMask().contains(objc2_app_kit::NSWindowStyleMask::FullScreen) {
            return;
        }
        let Some(screen) = window.screen() else { return };
        let (to, remember) = zoom_target(frame(window.frame()), frame(screen.visibleFrame()), REMEMBERED.get());
        REMEMBERED.set(remember);
        window.setFrame_display(rect(to), false);
    }
}

/// Set the buttons, and say what that came to. `None` where there is nothing
/// to do — not a Mac, the window not to be had, or full screen. Cheap enough
/// to call every frame, which it needs to be: AppKit sets the buttons back
/// where it wants them whenever it lays the band out again. Between frames
/// `by_hand::keep` does the same.
#[cfg(target_os = "macos")]
pub fn apply(window: &impl raw_window_handle::HasWindowHandle) -> Option<Placed> {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSTitlebarSeparatorStyle, NSView};
    use raw_window_handle::RawWindowHandle;

    MainThreadMarker::new()?;
    let handle = window.window_handle().ok()?;
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
        return None;
    };
    let view: &NSView = unsafe { appkit.ns_view.cast::<NSView>().as_ref() };
    let window = view.window()?;
    held_still::set(view);
    zooming::step(&window);
    window.setTitlebarSeparatorStyle(NSTitlebarSeparatorStyle::None);
    by_hand::keep(&window);
    let (end, middles, across) = by_hand::set(&window)?;
    Some(Placed { band: BY_HAND_MIDDLE * 2.0, buttons_end: end, middles, across })
}

#[cfg(not(target_os = "macos"))]
pub fn apply<W>(_window: &W) -> Option<Placed> {
    None
}

/// The system's own cursor for the gap between two columns, set straight
/// with AppKit, every frame the pointer is in the gap: the newer one for a
/// divider where the system has it, else the old left-and-right arrows.
/// egui asks for the same once, when the pointer arrives, and not again
/// while it stays — and on a Mac the arrow was back a moment later, with
/// the pointer still in the gap. Elsewhere egui's own is enough.
pub fn column_cursor() {
    #[cfg(target_os = "macos")]
    {
        use objc2::{sel, ClassType};
        use objc2_app_kit::NSCursor;
        let cursor = if NSCursor::class().responds_to(sel!(columnResizeCursor)) {
            NSCursor::columnResizeCursor()
        } else {
            #[allow(deprecated)]
            NSCursor::resizeLeftRightCursor()
        };
        cursor.set();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Frame = Frame { x: 0.0, y: 70.0, width: 1512.0, height: 875.0 };

    /// A window grows to the screen and remembers where it was; filling it,
    /// it goes back there — or, with nothing remembered, to the middle.
    #[test]
    fn a_zoom_goes_to_the_screen_and_back() {
        let small = Frame { x: 200.0, y: 300.0, width: 900.0, height: 600.0 };
        assert_eq!(zoom_target(small, SCREEN, None), (SCREEN, Some(small)));
        assert_eq!(zoom_target(SCREEN, SCREEN, Some(small)), (small, None));
        let (back, _) = zoom_target(SCREEN, SCREEN, None);
        assert!(back.width < SCREEN.width && !back.fills(SCREEN));
        assert!((back.x - SCREEN.x) * 2.0 + back.width - SCREEN.width < 1.0);
    }
}
