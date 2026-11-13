//! Notifications through Apple's UserNotifications framework, so the app is
//! listed under System Settings › Notifications and its alerts carry its
//! own name and icon. Only for apps running from a bundle (`.app`):
//! UserNotifications refuses processes without a bundle identifier, so
//! development builds fall back to `osascript` (see [`crate::notify`]).

use std::sync::atomic::{AtomicU8, Ordering};

/// Last answer to an authorization request: 0 unknown, 1 granted, 2 denied.
static STATUS: AtomicU8 = AtomicU8::new(0);

/// Whether the process runs from an app bundle.
#[cfg(target_os = "macos")]
pub fn available() -> bool {
    objc2_foundation::NSBundle::mainBundle()
        .bundleIdentifier()
        .is_some()
}

#[cfg(not(target_os = "macos"))]
pub fn available() -> bool {
    false
}

/// `Some(true)` once the user allowed notifications, `Some(false)` if they
/// declined, `None` before anyone asked in this run.
pub fn granted() -> Option<bool> {
    match STATUS.load(Ordering::Relaxed) {
        1 => Some(true),
        2 => Some(false),
        _ => None,
    }
}

/// Asks macOS for permission (a prompt the first time; afterwards it
/// answers with the user's choice without asking again).
#[cfg(target_os = "macos")]
pub fn request_authorization() {
    use objc2::runtime::Bool;
    use objc2_foundation::NSError;
    use objc2_user_notifications::{UNAuthorizationOptions, UNUserNotificationCenter};
    if !available() {
        return;
    }
    let handler = block2::RcBlock::new(|granted: Bool, _error: *mut NSError| {
        STATUS.store(if granted.as_bool() { 1 } else { 2 }, Ordering::Relaxed);
    });
    UNUserNotificationCenter::currentNotificationCenter()
        .requestAuthorizationWithOptions_completionHandler(
            UNAuthorizationOptions::Alert
                | UNAuthorizationOptions::Sound
                | UNAuthorizationOptions::Badge,
            &handler,
        );
}

#[cfg(not(target_os = "macos"))]
pub fn request_authorization() {}

/// Posts a notification as the app. Returns false when UserNotifications
/// can't be used (no bundle), so the caller falls back.
#[cfg(target_os = "macos")]
pub fn post(title: &str, subtitle: &str, body: &str) -> bool {
    use objc2_foundation::NSString;
    use objc2_user_notifications::{
        UNMutableNotificationContent, UNNotificationRequest, UNUserNotificationCenter,
    };
    if !available() {
        return false;
    }
    let content = UNMutableNotificationContent::new();
    content.setTitle(&NSString::from_str(title));
    if !subtitle.is_empty() {
        content.setSubtitle(&NSString::from_str(subtitle));
    }
    content.setBody(&NSString::from_str(body));
    let id = NSString::from_str(&format!(
        "ferrum-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos())
    ));
    let request = UNNotificationRequest::requestWithIdentifier_content_trigger(&id, &content, None);
    UNUserNotificationCenter::currentNotificationCenter()
        .addNotificationRequest_withCompletionHandler(&request, None);
    true
}

#[cfg(not(target_os = "macos"))]
pub fn post(_title: &str, _subtitle: &str, _body: &str) -> bool {
    false
}
