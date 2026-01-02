#![deny(unsafe_op_in_unsafe_fn)]

use crate::{AppState, frame, init};
use std::ptr;

use objc2::DefinedClass;

use std::{cell::RefCell, collections::HashSet, ptr::NonNull, sync::Mutex};

use objc2::define_class;
use objc2::runtime::ProtocolObject;

use objc2_foundation::{NSNotification, NSObject, NSObjectProtocol, NSSize};

use objc2_app_kit::{NSApplication, NSApplicationDelegate, NSEvent, NSEventMask, NSEventType};

use objc2_metal_kit::{MTKView, MTKViewDelegate};

use block2::RcBlock;
use once_cell::sync::Lazy;

// Global keystate - accessible from anywhere
pub static KEYSTATE: Lazy<Mutex<HashSet<u16>>> = Lazy::new(|| Mutex::new(HashSet::new()));

pub struct Ivars {
    pub state: RefCell<Option<AppState>>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = objc2::MainThreadOnly]
    #[ivars = Ivars]
    pub struct Delegate;

    unsafe impl NSObjectProtocol for Delegate {}

    unsafe impl NSApplicationDelegate for Delegate {
        #[unsafe(method(applicationDidFinishLaunching:))]
        unsafe fn init(&self, _notification: &NSNotification) {
            let (state, _window, view) = init();
            view.setDelegate(Some(ProtocolObject::from_ref(self)));
            *self.ivars().state.borrow_mut() = Some(state);

            let event_mask = NSEventMask::KeyDown | NSEventMask::KeyUp;
            let block = RcBlock::new(|event: NonNull<NSEvent>| -> *mut NSEvent {
                let event_ref = unsafe { event.as_ref() };
                let keycode = event_ref.keyCode();
                // https://doc.rust-lang.org/rust-by-example/compatibility/raw_identifiers.html
                match event_ref.r#type() {
                    NSEventType::KeyDown => {
                        KEYSTATE.lock().unwrap().insert(keycode);
                    }
                    NSEventType::KeyUp => {
                        KEYSTATE.lock().unwrap().remove(&keycode);
                    }
                    _ => {}
                }
                ptr::null_mut()
            });

            unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(event_mask, &block) };
        }

        #[unsafe(method(applicationShouldTerminateAfterLastWindowClosed:))]
        fn on_window_close(&self, _: &NSApplication) -> bool {
            true
        }
    }

    unsafe impl MTKViewDelegate for Delegate {
        #[unsafe(method(drawInMTKView:))]
        unsafe fn draw(&self, mtk_view: &MTKView) {
            let state_ref = self.ivars().state.borrow();
            if let Some(state) = state_ref.as_ref() {
                frame(mtk_view, state);
            }
        }

        #[unsafe(method(mtkView:drawableSizeWillChange:))]
        unsafe fn update_view_on_resize(&self, _view: &MTKView, _size: NSSize) {}
    }
);
