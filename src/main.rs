mod window_search;

use std::cell::RefCell;
use std::ptr::NonNull;
use std::rc::Rc;
use std::time::Instant;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{class, msg_send, MainThreadMarker};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSBackingStoreType, NSColor, NSPanel, NSScreen,
    NSTextField, NSView, NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString, NSTimer};

use window_search::{get_all_windows, get_ignored_apps};

struct MaskingPanel {
    panel: Retained<NSPanel>,
    mask_view: Retained<NSView>,
    ignored_apps: std::collections::HashSet<String>,
    our_window_number: RefCell<i64>,
    update_timer: RefCell<Option<Retained<NSTimer>>>,
}

impl MaskingPanel {
    fn get_mask_color(&self, bundle_id: &str) -> (f64, f64, f64, f64) {
        match bundle_id {
            "net.kovidgoyal.kitty" => (0.5, 0.0, 0.5, 0.7),
            "com.google.Chrome" => (0.0, 0.0, 0.0, 0.5),
            "com.hnc.DiscordPTB" => (0.0, 0.0, 1.0, 0.9),
            _ => (0.0, 0.0, 0.0, 1.0),
        }
    }

    fn new(mtm: MainThreadMarker) -> Option<Rc<Self>> {
        unsafe {
            let _screen = NSScreen::mainScreen(mtm)?;

            let panel = NSPanel::initWithContentRect_styleMask_backing_defer(
                mtm.alloc(),
                NSRect::new(NSPoint::new(100.0, 100.0), NSSize::new(800.0, 600.0)),
                NSWindowStyleMask::Borderless,
                NSBackingStoreType::Buffered,
                false,
            );

            panel.setLevel(10);
            panel.setOpaque(false);
            panel.setBackgroundColor(Some(&NSColor::clearColor()));
            panel.setHasShadow(true);
            panel.setMovableByWindowBackground(true);
            panel.setHidesOnDeactivate(false);
            panel.setCollectionBehavior(
                NSWindowCollectionBehavior::CanJoinAllSpaces
                    | NSWindowCollectionBehavior::Stationary,
            );
            panel.setTitle(&NSString::from_str("Screen Masking"));

            let content_view = NSView::initWithFrame(
                mtm.alloc(),
                NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(800.0, 600.0)),
            );
            panel.setContentView(Some(&content_view));

            let mask_view = NSView::initWithFrame(
                mtm.alloc(),
                NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(800.0, 600.0)),
            );
            mask_view.setWantsLayer(true);

            if let Some(layer) = mask_view.layer() {
                let bg_color = NSColor::colorWithRed_green_blue_alpha(1.0, 1.0, 1.0, 0.3);
                let cg_color = bg_color.CGColor();
                layer.setBackgroundColor(Some(&cg_color));
            }

            content_view.addSubview(&mask_view);

            let window_number: i64 = msg_send![&panel, windowNumber];

            let panel_obj = Rc::new(Self {
                panel: panel.clone(),
                mask_view,
                ignored_apps: get_ignored_apps(),
                our_window_number: RefCell::new(window_number),
                update_timer: RefCell::new(None),
            });

            panel.makeKeyAndOrderFront(None);
            panel.orderFrontRegardless();

            panel_obj.update_masks();

            let panel_clone = Rc::clone(&panel_obj);
            let notification_center: Retained<AnyObject> =
                msg_send![class!(NSNotificationCenter), defaultCenter];
            let move_name = NSString::from_str("NSWindowDidMoveNotification");

            let block = block2::RcBlock::new(move |_: NonNull<AnyObject>| {
                panel_clone.schedule_update();
            });

            let _observer: Retained<AnyObject> = msg_send![
                &notification_center,
                addObserverForName: &*move_name,
                object: &*panel,
                queue: std::ptr::null::<AnyObject>(),
                usingBlock: &*block
            ];

            Some(panel_obj)
        }
    }

    fn schedule_update(&self) {
        if let Some(timer) = self.update_timer.borrow_mut().take() {
            timer.invalidate();
        }

        let self_ptr = self as *const Self;

        let block = block2::RcBlock::new(move |_: NonNull<NSTimer>| {
            unsafe {
                let panel = &*self_ptr;
                panel.update_masks();
                *panel.update_timer.borrow_mut() = None;
            }
        });

        unsafe {
            let timer: Retained<NSTimer> = msg_send![
                class!(NSTimer),
                scheduledTimerWithTimeInterval: 0.3,
                repeats: false,
                block: &*block
            ];
            *self.update_timer.borrow_mut() = Some(timer);
        }
    }

    fn update_masks(&self) {
        let start = Instant::now();

        let subviews = self.mask_view.subviews();
        for subview in subviews.to_vec() {
            subview.removeFromSuperview();
        }

        let our_frame = self.panel.frame();
        let our_x = our_frame.origin.x;
        let our_y = our_frame.origin.y;

        let mtm = MainThreadMarker::new().unwrap();
        let screen = NSScreen::mainScreen(mtm).unwrap();
        let screen_height = screen.frame().size.height;

        if let Ok(windows) = get_all_windows(&self.ignored_apps) {
            let mut masked_count = 0;
            let our_wnum = *self.our_window_number.borrow();

            for window in windows {
                if window.window_number == our_wnum {
                    continue;
                }

                let window_ns_y = screen_height - window.y - window.height;
                let rel_x = window.x - our_x;
                let rel_y = window_ns_y - our_y;

                if rel_x + window.width > 0.0
                    && rel_x < our_frame.size.width
                    && rel_y + window.height > 0.0
                    && rel_y < our_frame.size.height
                {
                    masked_count += 1;

                    let mask_rect = NSRect::new(
                        NSPoint::new(rel_x, rel_y),
                        NSSize::new(window.width, window.height),
                    );

                    unsafe {
                        let mask = NSView::initWithFrame(mtm.alloc(), mask_rect);
                        mask.setWantsLayer(true);

                        if let Some(layer) = mask.layer() {
                            let bundle_id = window.bundle_identifier.as_deref().unwrap_or("");
                            let (r, g, b, a) = self.get_mask_color(bundle_id);
                            let mask_color = NSColor::colorWithRed_green_blue_alpha(r, g, b, a);
                            let cg_color = mask_color.CGColor();
                            layer.setBackgroundColor(Some(&cg_color));
                        }

                        let label_text = NSString::from_str(
                            &window
                                .bundle_identifier
                                .as_ref()
                                .unwrap_or(&window.app_name),
                        );
                        let label = NSTextField::labelWithString(&label_text, mtm);

                        label.setFrame(NSRect::new(
                            NSPoint::new(10.0, window.height / 2.0 - 10.0),
                            NSSize::new(window.width - 20.0, 20.0),
                        ));
                        label.setTextColor(Some(&NSColor::whiteColor()));
                        label.setBackgroundColor(Some(&NSColor::clearColor()));

                        mask.addSubview(&label);
                        self.mask_view.addSubview(&mask);
                    }
                }
            }

            println!("Masked: {}, Time: {:?}", masked_count, start.elapsed());
        }
    }
}

fn main() {
    let mtm = MainThreadMarker::new().unwrap();
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

    let _panel = MaskingPanel::new(mtm).expect("Failed to create masking panel");

    app.run();
}
