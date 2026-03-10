use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::Bool;
use objc2_foundation::{NSBundle, NSError, NSString};
use objc2_user_notifications::{
  UNAuthorizationOptions, UNMutableNotificationContent, UNNotificationRequest, UNUserNotificationCenter,
};

/// Requests notification authorization. Call once on startup.
pub fn request_authorization() {
  if NSBundle::mainBundle().bundleIdentifier().is_none() {
    log::debug!("Skipping notification authorization (no app bundle)");
    return;
  }

  log::info!("Requesting notification authorization");

  let center = UNUserNotificationCenter::currentNotificationCenter();
  let handler = RcBlock::new(|granted: Bool, error: *mut NSError| {
    if !error.is_null() {
      let error = unsafe { Retained::retain(error) }.unwrap();
      log::warn!("Notification authorization error: {error}");
    } else if granted.as_bool() {
      log::info!("Notification authorization granted");
    } else {
      log::info!("Notification authorization denied");
    }
  });

  center.requestAuthorizationWithOptions_completionHandler(
    UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound,
    &handler,
  );
}

/// Posts a macOS notification with the given title and body.
/// Silently skips if the app is not running from a bundle (e.g. cargo run).
pub fn send(title: &str, body: &str) {
  if NSBundle::mainBundle().bundleIdentifier().is_none() {
    return;
  }

  let content = UNMutableNotificationContent::new();
  content.setTitle(&NSString::from_str(title));
  content.setBody(&NSString::from_str(body));

  let id = NSString::from_str(&format!(
    "liment-{}",
    std::time::SystemTime::now()
      .duration_since(std::time::UNIX_EPOCH)
      .unwrap_or_default()
      .as_millis()
  ));

  let request = UNNotificationRequest::requestWithIdentifier_content_trigger(&id, &content, None);
  let center = UNUserNotificationCenter::currentNotificationCenter();
  center.addNotificationRequest_withCompletionHandler(&request, None);
}

/// Posts an error notification.
pub fn send_error(body: &str) {
  send("Liment", body);
}
