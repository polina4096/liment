use objc2::{MainThreadMarker, Message, rc::Retained};
use objc2_app_kit::{
  NSColor, NSFont, NSLayoutConstraint, NSMenuItem, NSProgressIndicator, NSProgressIndicatorStyle, NSTextField, NSView,
};
use objc2_core_foundation::CGFloat;
use objc2_foundation::{NSArray, NSSize, NSString};

use crate::{
  api::{SubscriptionTier, UsageBucket},
  util::{NSViewExt, format_reset_time},
};

fn font_weight_regular() -> CGFloat {
  return unsafe { objc2_app_kit::NSFontWeightRegular };
}

fn font_weight_medium() -> CGFloat {
  return unsafe { objc2_app_kit::NSFontWeightMedium };
}

pub fn font_weight_semibold() -> CGFloat {
  return unsafe { objc2_app_kit::NSFontWeightSemibold };
}

fn font_weight_light() -> CGFloat {
  return unsafe { objc2_app_kit::NSFontWeightLight };
}

fn activate(constraints: &[&NSLayoutConstraint]) {
  let array = NSArray::from_retained_slice(&constraints.iter().map(|c| c.retain()).collect::<Vec<_>>());

  return NSLayoutConstraint::activateConstraints(&array);
}

pub fn bucket_row(mtm: MainThreadMarker, label: &str, bucket: &UsageBucket) -> Retained<NSMenuItem> {
  let reset_str = format_reset_time(&bucket.resets_at);
  let view = progress_row(mtm, label, bucket.utilization, &reset_str);
  let item = NSMenuItem::new(mtm);
  item.setView(Some(&view));

  return item;
}

pub fn progress_row(mtm: MainThreadMarker, label: &str, utilization: f64, reset_str: &str) -> Retained<NSView> {
  let container = NSView::init(mtm.alloc::<NSView>());
  container.setFrameSize(NSSize::new(280.0, 48.0));

  // Label: "5h Limit  8%".
  let label_text = format!("{}  {}%", label, utilization as u32);
  let label_field = NSTextField::labelWithString(&NSString::from_str(&label_text), mtm);
  label_field.noAutoresize();
  label_field.setEditable(false);
  label_field.setBezeled(false);
  label_field.setDrawsBackground(false);

  let font = NSFont::systemFontOfSize_weight(12.0, font_weight_regular());
  label_field.setFont(Some(&font));
  container.addSubview(&label_field);

  // Reset time label (right-aligned).
  let reset_text = format!("resets in {}", reset_str);
  let reset_field = NSTextField::labelWithString(&NSString::from_str(&reset_text), mtm);
  reset_field.noAutoresize();
  reset_field.setEditable(false);
  reset_field.setBezeled(false);
  reset_field.setDrawsBackground(false);

  let small_font = NSFont::systemFontOfSize_weight(10.0, font_weight_light());
  reset_field.setFont(Some(&small_font));
  reset_field.setAlignment(objc2_app_kit::NSTextAlignment::Right);
  reset_field.setTextColor(Some(&NSColor::secondaryLabelColor()));
  container.addSubview(&reset_field);

  // Progress bar.
  let progress = NSProgressIndicator::init(mtm.alloc::<NSProgressIndicator>());
  progress.noAutoresize();
  progress.setStyle(NSProgressIndicatorStyle::Bar);
  progress.setIndeterminate(false);
  progress.setMinValue(0.0);
  progress.setMaxValue(100.0);
  progress.setDoubleValue(utilization);
  container.addSubview(&progress);

  activate(&[
    // Container width.
    &container.widthAnchor().constraintEqualToConstant(280.0),
    // Label row: top, leading, trailing.
    &label_field.topAnchor().constraintEqualToAnchor_constant(&container.topAnchor(), 4.0),
    &label_field.leadingAnchor().constraintEqualToAnchor_constant(&container.leadingAnchor(), 14.0),
    &label_field.trailingAnchor().constraintEqualToAnchor_constant(&container.trailingAnchor(), -14.0),
    // Reset label: same row as label, right-aligned.
    &reset_field.topAnchor().constraintEqualToAnchor(&label_field.topAnchor()),
    &reset_field.leadingAnchor().constraintEqualToAnchor(&label_field.leadingAnchor()),
    &reset_field.trailingAnchor().constraintEqualToAnchor(&label_field.trailingAnchor()),
    // Progress bar: below label, pinned to sides.
    &progress.topAnchor().constraintEqualToAnchor_constant(&label_field.bottomAnchor(), 4.0),
    &progress.leadingAnchor().constraintEqualToAnchor_constant(&container.leadingAnchor(), 14.0),
    &progress.trailingAnchor().constraintEqualToAnchor_constant(&container.trailingAnchor(), -14.0),
    &progress.heightAnchor().constraintEqualToConstant(14.0),
    // Container bottom.
    &container.bottomAnchor().constraintEqualToAnchor_constant(&progress.bottomAnchor(), 6.0),
  ]);

  return container;
}

fn tier_badge_color(tier: SubscriptionTier) -> Retained<NSColor> {
  return match tier {
    SubscriptionTier::Free => NSColor::colorWithSRGBRed_green_blue_alpha(0.60, 0.60, 0.60, 1.0),
    SubscriptionTier::Pro => NSColor::colorWithSRGBRed_green_blue_alpha(0.30, 0.55, 0.90, 1.0),
    SubscriptionTier::Max5x => NSColor::colorWithSRGBRed_green_blue_alpha(0.55, 0.35, 0.85, 1.0),
    SubscriptionTier::Max20x => NSColor::colorWithSRGBRed_green_blue_alpha(0.85, 0.45, 0.20, 1.0),
  };
}

pub fn header_row(mtm: MainThreadMarker, title: &str, tier: Option<SubscriptionTier>) -> Retained<NSView> {
  let container = NSView::init(mtm.alloc::<NSView>());
  container.setFrameSize(NSSize::new(280.0, 28.0));

  // Title label.
  let field = NSTextField::labelWithString(&NSString::from_str(title), mtm);
  field.setEditable(false);
  field.setBezeled(false);
  field.setDrawsBackground(false);

  let font = NSFont::systemFontOfSize_weight(14.0, font_weight_semibold());
  field.noAutoresize();
  field.setFont(Some(&font));
  container.addSubview(&field);

  activate(&[
    &container.widthAnchor().constraintEqualToConstant(280.0),
    &field.leadingAnchor().constraintEqualToAnchor_constant(&container.leadingAnchor(), 14.0),
    &field.centerYAnchor().constraintEqualToAnchor(&container.centerYAnchor()),
  ]);

  // Tier badge.
  if let Some(tier) = tier {
    let badge_font = NSFont::systemFontOfSize_weight(10.0, font_weight_medium());
    let badge_height: CGFloat = 18.0;

    let badge_view = NSView::init(mtm.alloc::<NSView>());
    badge_view.noAutoresize();
    badge_view.setWantsLayer(true);

    if let Some(layer) = badge_view.layer() {
      let color = tier_badge_color(tier);
      layer.setBackgroundColor(Some(&color.CGColor()));
      layer.setCornerRadius(badge_height / 2.0);
    }

    container.addSubview(&badge_view);

    let tier_str = tier.to_string();
    let badge_label = NSTextField::labelWithString(&NSString::from_str(&tier_str), mtm);
    badge_label.noAutoresize();
    badge_label.setEditable(false);
    badge_label.setBezeled(false);
    badge_label.setDrawsBackground(false);
    badge_label.setFont(Some(&badge_font));
    badge_label.setTextColor(Some(&NSColor::whiteColor()));
    badge_label.setAlignment(objc2_app_kit::NSTextAlignment::Center);
    badge_view.addSubview(&badge_label);

    activate(&[
      // Badge view: next to title, vertically centered.
      &badge_view.leadingAnchor().constraintEqualToAnchor_constant(&field.trailingAnchor(), 8.0),
      &badge_view.centerYAnchor().constraintEqualToAnchor(&container.centerYAnchor()),
      &badge_view.heightAnchor().constraintEqualToConstant(badge_height),
      // Badge label fills badge view with horizontal padding.
      &badge_label.leadingAnchor().constraintEqualToAnchor_constant(&badge_view.leadingAnchor(), 6.0),
      &badge_label.trailingAnchor().constraintEqualToAnchor_constant(&badge_view.trailingAnchor(), -6.0),
      &badge_label.centerYAnchor().constraintEqualToAnchor_constant(&badge_view.centerYAnchor(), -1.0),
    ]);
  }

  activate(&[&container.heightAnchor().constraintEqualToConstant(28.0)]);

  return container;
}

pub fn label_row(mtm: MainThreadMarker, text: &str, bold: bool) -> Retained<NSView> {
  let container = NSView::init(mtm.alloc::<NSView>());
  container.setFrameSize(NSSize::new(280.0, 22.0));

  let field = NSTextField::labelWithString(&NSString::from_str(text), mtm);
  field.setEditable(false);
  field.setBezeled(false);
  field.setDrawsBackground(false);

  let weight = if bold { font_weight_semibold() } else { font_weight_regular() };
  let font = NSFont::systemFontOfSize_weight(12.0, weight);
  field.noAutoresize();
  field.setFont(Some(&font));

  container.addSubview(&field);

  activate(&[
    &container.widthAnchor().constraintEqualToConstant(280.0),
    &container.heightAnchor().constraintEqualToConstant(22.0),
    &field.leadingAnchor().constraintEqualToAnchor_constant(&container.leadingAnchor(), 14.0),
    &field.trailingAnchor().constraintEqualToAnchor_constant(&container.trailingAnchor(), -14.0),
    &field.centerYAnchor().constraintEqualToAnchor(&container.centerYAnchor()),
  ]);

  return container;
}

pub fn key_value_row(mtm: MainThreadMarker, key: &str, value: &str) -> Retained<NSView> {
  let container = NSView::init(mtm.alloc::<NSView>());
  container.setFrameSize(NSSize::new(280.0, 22.0));

  let key_field = NSTextField::labelWithString(&NSString::from_str(key), mtm);
  key_field.setEditable(false);
  key_field.setBezeled(false);
  key_field.setDrawsBackground(false);

  let font = NSFont::systemFontOfSize_weight(12.0, font_weight_regular());
  key_field.noAutoresize();
  key_field.setFont(Some(&font));
  container.addSubview(&key_field);

  let value_field = NSTextField::labelWithString(&NSString::from_str(value), mtm);
  value_field.noAutoresize();
  value_field.setEditable(false);
  value_field.setBezeled(false);
  value_field.setDrawsBackground(false);
  value_field.setFont(Some(&font));
  value_field.setAlignment(objc2_app_kit::NSTextAlignment::Right);
  value_field.setTextColor(Some(&NSColor::secondaryLabelColor()));
  container.addSubview(&value_field);

  activate(&[
    &container.widthAnchor().constraintEqualToConstant(280.0),
    &container.heightAnchor().constraintEqualToConstant(22.0),
    &key_field.leadingAnchor().constraintEqualToAnchor_constant(&container.leadingAnchor(), 14.0),
    &key_field.centerYAnchor().constraintEqualToAnchor(&container.centerYAnchor()),
    &value_field.trailingAnchor().constraintEqualToAnchor_constant(&container.trailingAnchor(), -14.0),
    &value_field.centerYAnchor().constraintEqualToAnchor(&container.centerYAnchor()),
  ]);

  return container;
}
