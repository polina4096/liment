use objc2::{AnyThread, MainThreadMarker, MainThreadOnly, Message, define_class, rc::Retained, sel};
use objc2_app_kit::{
  NSButton, NSColor, NSCursor, NSFont, NSFontAttributeName, NSFontWeightRegular, NSForegroundColorAttributeName,
  NSImage, NSImageView, NSLayoutAttribute, NSLayoutConstraint, NSStackView, NSTextAlignment, NSTextField,
  NSUserInterfaceLayoutOrientation, NSView, NSVisualEffectBlendingMode, NSVisualEffectMaterial, NSVisualEffectView,
  NSWindow, NSWindowStyleMask,
};
use objc2_core_foundation::CGPoint;
use objc2_foundation::{NSArray, NSMutableAttributedString, NSObjectProtocol, NSRange, NSRect, NSSize, NSString};

use super::components::font_weight_semibold;
use crate::{delegate::AppDelegate, utils::macos::NSViewExt};

fn activate(constraints: &[&NSLayoutConstraint]) {
  let array = NSArray::from_retained_slice(&constraints.iter().map(|c| c.retain()).collect::<Vec<_>>());
  NSLayoutConstraint::activateConstraints(&array);
}

pub fn build_about_window(mtm: MainThreadMarker, app: &AppDelegate) -> Retained<NSWindow> {
  let style = NSWindowStyleMask::Titled
    .union(NSWindowStyleMask::Closable)
    .union(NSWindowStyleMask::FullSizeContentView);

  let window = unsafe {
    NSWindow::initWithContentRect_styleMask_backing_defer(
      mtm.alloc::<NSWindow>(),
      NSRect::new(CGPoint::new(0.0, 0.0), NSSize::new(256.0, 100.0)),
      style,
      objc2_app_kit::NSBackingStoreType(2), // NSBackingStoreBuffered
      false,
    )
  };

  window.setTitlebarAppearsTransparent(true);
  window.setTitle(&NSString::new());
  window.setMovableByWindowBackground(true);
  unsafe { window.setReleasedWhenClosed(false) };

  // Visual effect background.
  let effect = NSVisualEffectView::initWithFrame(mtm.alloc::<NSVisualEffectView>(), NSRect::ZERO);
  effect.setMaterial(NSVisualEffectMaterial::HUDWindow);
  effect.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
  effect.setState(objc2_app_kit::NSVisualEffectState::Active);
  window.setContentView(Some(&effect));

  // Main vertical stack.
  let stack = NSStackView::initWithFrame(mtm.alloc::<NSStackView>(), NSRect::ZERO);
  stack.noAutoresize();
  stack.setOrientation(NSUserInterfaceLayoutOrientation::Vertical);
  stack.setAlignment(NSLayoutAttribute::CenterX);
  stack.setSpacing(8.0);
  effect.addSubview(&stack);

  // App icon.
  let icon_view = NSImageView::initWithFrame(mtm.alloc::<NSImageView>(), NSRect::ZERO);
  icon_view.noAutoresize();
  if let Some(icon) = NSImage::imageNamed(&NSString::from_str("NSApplicationIcon")) {
    icon_view.setImage(Some(&icon));
  }
  stack.addArrangedSubview(&icon_view);

  // Title.
  let title = NSTextField::labelWithString(&NSString::from_str("liment"), mtm);
  title.setSelectable(true);
  title.setFont(Some(&NSFont::systemFontOfSize_weight(18.0, font_weight_semibold())));
  stack.addArrangedSubview(&title);

  // Description.
  let desc = NSTextField::wrappingLabelWithString(&NSString::from_str(env!("CARGO_PKG_DESCRIPTION")), mtm);
  desc.noAutoresize();
  desc.setSelectable(true);
  desc.setAlignment(NSTextAlignment::Center);
  desc.setFont(Some(&NSFont::systemFontOfSize_weight(11.0, unsafe { NSFontWeightRegular })));
  desc.setTextColor(Some(&NSColor::secondaryLabelColor()));
  desc.setPreferredMaxLayoutWidth(180.0);
  stack.addArrangedSubview(&desc);

  // Version / commit info.
  let info = NSStackView::initWithFrame(mtm.alloc::<NSStackView>(), NSRect::ZERO);
  info.setOrientation(NSUserInterfaceLayoutOrientation::Vertical);
  info.setAlignment(NSLayoutAttribute::CenterX);
  info.setSpacing(2.0);
  info.addArrangedSubview(&property_row(mtm, "Version", env!("CARGO_PKG_VERSION")));
  info.addArrangedSubview(&property_row(mtm, "Commit", env!("GIT_COMMIT_SHORT")));
  stack.addArrangedSubview(&info);

  // Links.
  let links = NSStackView::initWithFrame(mtm.alloc::<NSStackView>(), NSRect::ZERO);
  links.setOrientation(NSUserInterfaceLayoutOrientation::Horizontal);
  links.setSpacing(8.0);
  links.addArrangedSubview(&link_button(mtm, "Issues", app, sel!(onOpenIssues:)));
  links.addArrangedSubview(&link_button(mtm, "Source Code", app, sel!(onOpenSource:)));
  stack.addArrangedSubview(&links);

  // Stack constraints.
  activate(&[
    &stack.topAnchor().constraintEqualToAnchor_constant(&effect.topAnchor(), 48.0),
    &stack.centerXAnchor().constraintEqualToAnchor(&effect.centerXAnchor()),
    &effect.bottomAnchor().constraintEqualToAnchor_constant(&stack.bottomAnchor(), 36.0),
    &icon_view.widthAnchor().constraintEqualToConstant(64.0),
    &icon_view.heightAnchor().constraintEqualToConstant(64.0),
  ]);

  // Custom spacing between sections.
  stack.setCustomSpacing_afterView(24.0, &desc);
  stack.setCustomSpacing_afterView(24.0, &info);

  window.center();

  window
}

/// Two-column property row with right-aligned label and left-aligned monospaced value,
/// using fixed widths so the pair is centered as a unit within the parent stack.
fn property_row(mtm: MainThreadMarker, label: &str, value: &str) -> Retained<NSView> {
  let row = NSStackView::initWithFrame(mtm.alloc::<NSStackView>(), NSRect::ZERO);
  row.setOrientation(NSUserInterfaceLayoutOrientation::Horizontal);
  row.setSpacing(4.0);

  let label_field = NSTextField::labelWithString(&NSString::from_str(label), mtm);
  label_field.noAutoresize();
  label_field.setSelectable(true);
  label_field.setAlignment(NSTextAlignment::Right);
  label_field.setFont(Some(&NSFont::systemFontOfSize_weight(11.0, unsafe { NSFontWeightRegular })));
  row.addArrangedSubview(&label_field);

  let value_field = NSTextField::labelWithString(&NSString::from_str(value), mtm);
  value_field.noAutoresize();
  value_field.setSelectable(true);
  value_field.setFont(Some(&NSFont::monospacedSystemFontOfSize_weight(11.0, unsafe { NSFontWeightRegular })));
  value_field.setTextColor(Some(&NSColor::secondaryLabelColor()));
  row.addArrangedSubview(&value_field);

  activate(&[
    &label_field.widthAnchor().constraintEqualToConstant(100.0),
    &value_field.widthAnchor().constraintEqualToConstant(100.0),
  ]);

  Retained::into_super(row)
}

define_class!(
  #[unsafe(super(NSButton))]
  #[thread_kind = MainThreadOnly]
  #[name = "LinkButton"]
  struct LinkButton;

  impl LinkButton {
    #[unsafe(method(resetCursorRects))]
    fn reset_cursor_rects(&self) {
      self.addCursorRect_cursor(self.bounds(), &NSCursor::pointingHandCursor());
    }
  }

  unsafe impl NSObjectProtocol for LinkButton {}
);

/// Borderless button styled as a link (blue text, pointer cursor on hover).
fn link_button(mtm: MainThreadMarker, text: &str, app: &AppDelegate, action: objc2::runtime::Sel) -> Retained<NSView> {
  let btn: Retained<LinkButton> = unsafe { objc2::msg_send![mtm.alloc::<LinkButton>(), init] };
  btn.setBordered(false);
  btn.setRefusesFirstResponder(true);

  let font = NSFont::systemFontOfSize_weight(12.0, unsafe { NSFontWeightRegular });
  let color = NSColor::linkColor();
  let ns_str = NSString::from_str(text);

  let attr = NSMutableAttributedString::initWithString(NSMutableAttributedString::alloc(), &ns_str);
  let range = NSRange::new(0, text.encode_utf16().count());
  unsafe {
    attr.addAttribute_value_range(NSFontAttributeName, &font, range);
    attr.addAttribute_value_range(NSForegroundColorAttributeName, &color, range);
  }
  btn.setAttributedTitle(&Retained::into_super(attr));

  unsafe { btn.setTarget(Some(app)) };
  unsafe { btn.setAction(Some(action)) };

  Retained::into_super(Retained::into_super(Retained::into_super(btn)))
}
