#![allow(deprecated)]

use cocoa::appkit::NSApp;
use cocoa::base::nil;
use cocoa::foundation::NSString;
use objc::runtime::Object;
use objc::*;
use std::ffi::c_void;

extern "C" {
    static _dispatch_main_q: c_void;
    fn dispatch_async_f(
        queue: *const c_void,
        context: *mut c_void,
        work: extern "C" fn(*mut c_void),
    );
}

extern "C" fn set_badge_on_main(context: *mut c_void) {
    let count = context as usize;
    unsafe {
        let app: *mut Object = NSApp();
        let dock_tile: *mut Object = msg_send![app, dockTile];
        let label = if count > 0 {
            NSString::alloc(nil).init_str(&count.to_string())
        } else {
            nil
        };
        let _: () = msg_send![dock_tile, setBadgeLabel: label];
    }
}

pub fn set_badge_count(count: usize) {
    unsafe {
        dispatch_async_f(
            &_dispatch_main_q as *const c_void,
            count as *mut c_void,
            set_badge_on_main,
        );
    }
}
