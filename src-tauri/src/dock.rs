#![allow(deprecated)]

use cocoa::appkit::NSApp;
use cocoa::base::nil;
use cocoa::foundation::NSString;
use objc::runtime::Object;
use objc::*;

pub fn set_badge_count(count: usize) {
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
