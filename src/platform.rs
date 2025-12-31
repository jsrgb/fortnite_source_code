#![deny(unsafe_op_in_unsafe_fn)]

use crate::{frame, init, AppState};

use objc2::DefinedClass;

use std::cell::RefCell;

use objc2::define_class;
use objc2::runtime::ProtocolObject;

use objc2_foundation::{NSNotification, NSObject, NSObjectProtocol, NSSize};

use objc2_app_kit::{NSApplication, NSApplicationDelegate};

use objc2_metal_kit::{MTKView, MTKViewDelegate};

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
            let (state, window, view) = init();
            view.setDelegate(Some(ProtocolObject::from_ref(self)));
            *self.ivars().state.borrow_mut() = Some(state);
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
