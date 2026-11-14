pub const BY_HAND_MIDDLE: f32 = 28.0;

#[cfg(target_os = "macos")]
const BY_HAND_LEFT: f64 = 20.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placed {
    pub band: f32,
    pub buttons_end: f32,
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

    fn band_view(button: &objc2_app_kit::NSButton) -> Option<objc2::rc::Retained<objc2_app_kit::NSView>> {
        unsafe { button.superview()?.superview() }
    }

    pub fn set(window: &NSWindow) -> Option<f32> {
        use objc2_app_kit::NSWindowStyleMask;
        if window.styleMask().contains(NSWindowStyleMask::FullScreen) {
            return None;
        }
        let [close, shrink, zoom] = buttons(window)?;
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
        for (at, button) in [&close, &shrink, &zoom].into_iter().enumerate() {
            let size = button.frame().size;
            let origin = NSPoint::new(super::BY_HAND_LEFT + step * at as f64, (height - size.height) / 2.0);
            if button.frame().origin != origin {
                button.setFrameOrigin(origin);
            }
            end = origin.x + size.width;
        }
        Some(end as f32)
    }

    thread_local! {
        static WATCHED: std::cell::RefCell<Vec<*const objc2::runtime::AnyObject>> =
            const { std::cell::RefCell::new(Vec::new()) };
        static SETTING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
        static AGAIN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
        static STEP: std::cell::Cell<Option<f64>> = const { std::cell::Cell::new(None) };
    }

    const STANDARD_STEP: f64 = 20.0;

    const AT_MOST: usize = 8;

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
    fn fills(self, screen: Frame) -> bool {
        (self.x - screen.x).abs() < 2.0
            && (self.y - screen.y).abs() < 2.0
            && (self.width - screen.width).abs() < 2.0
            && (self.height - screen.height).abs() < 2.0
    }

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

#[cfg(any(target_os = "macos", test))]
fn zoom_target(window: Frame, screen: Frame, remembered: Option<Frame>) -> (Frame, Option<Frame>) {
    if window.fills(screen) {
        (remembered.unwrap_or_else(|| Frame::middle_of(screen)), None)
    } else {
        (screen, Some(window))
    }
}

pub fn zoom() {
    #[cfg(target_os = "macos")]
    zooming::ASKED.with(|asked| asked.set(true));
}

#[cfg(target_os = "macos")]
mod zooming {
    use super::{zoom_target, Frame};
    use objc2_app_kit::NSWindow;
    use objc2_foundation::{NSPoint, NSRect, NSSize};
    use std::cell::Cell;

    thread_local! {
        pub static ASKED: Cell<bool> = const { Cell::new(false) };
        static REMEMBERED: Cell<Option<Frame>> = const { Cell::new(None) };
    }

    fn frame(rect: NSRect) -> Frame {
        Frame { x: rect.origin.x, y: rect.origin.y, width: rect.size.width, height: rect.size.height }
    }

    fn rect(frame: Frame) -> NSRect {
        NSRect::new(NSPoint::new(frame.x, frame.y), NSSize::new(frame.width, frame.height))
    }

    pub fn step(window: &NSWindow) {
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
    let end = by_hand::set(&window)?;
    Some(Placed { band: BY_HAND_MIDDLE * 2.0, buttons_end: end })
}

#[cfg(not(target_os = "macos"))]
pub fn apply<W>(_window: &W) -> Option<Placed> {
    None
}

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
