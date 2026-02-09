use objc2::{MainThreadMarker, Message, rc::Retained};
use objc2_app_kit::{
  NSColor, NSFont, NSLayoutConstraint, NSMenuItem, NSProgressIndicator, NSProgressIndicatorStyle, NSTextField, NSView,
};
use objc2_core_foundation::CGFloat;
use objc2_foundation::{NSArray, NSString};

use crate::utils::macos::NSViewExt;
use crate::utils::time::{format_absolute_time, format_reset_time};

use jiff::Timestamp;

const MENU_WIDTH: CGFloat = 256.0;
const H_PADDING: CGFloat = 14.0;

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

/// Resolves Auto Layout constraints and updates the container's frame.
fn layout(container: &NSView) {
  container.layoutSubtreeIfNeeded();
  container.setFrameSize(container.fittingSize());
}

pub fn bucket_row(mtm: MainThreadMarker, label: &str, utilization: f64, resets_at: &Timestamp, period_seconds: Option<i64>, absolute_time: bool, is_remaining: bool) -> Retained<NSMenuItem> {
  let mut reset_str = if absolute_time {
    format!("reset: {}", format_absolute_time(resets_at))
  } else {
    format!("resets in {}", format_reset_time(resets_at))
  };

  if let Some(period) = period_seconds {
    let now = Timestamp::now();
    let remaining = resets_at.as_second() - now.as_second();
    if remaining > 0 && period > 0 {
      let elapsed_pct = ((period - remaining) as f64 / period as f64 * 100.0).clamp(0.0, 100.0);
      let display_pct = if is_remaining { 100.0 - elapsed_pct } else { elapsed_pct };
      reset_str = format!("{} ({:.0}%)", reset_str, display_pct);
    }
  }

  let view = progress_row(mtm, label, utilization, &reset_str);
  let item = NSMenuItem::new(mtm);
  item.setView(Some(&view));

  return item;
}

pub fn progress_row(mtm: MainThreadMarker, label: &str, utilization: f64, reset_str: &str) -> Retained<NSView> {
  let container = NSView::init(mtm.alloc::<NSView>());

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
  let reset_field = NSTextField::labelWithString(&NSString::from_str(reset_str), mtm);
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
    &container.widthAnchor().constraintEqualToConstant(MENU_WIDTH),
    // Label row: top, leading, trailing.
    &label_field.topAnchor().constraintEqualToAnchor_constant(&container.topAnchor(), 6.0),
    &label_field.leadingAnchor().constraintEqualToAnchor_constant(&container.leadingAnchor(), H_PADDING),
    &label_field
      .trailingAnchor()
      .constraintEqualToAnchor_constant(&container.trailingAnchor(), -H_PADDING),
    // Reset label: same row as label, right-aligned.
    &reset_field.topAnchor().constraintEqualToAnchor(&label_field.topAnchor()),
    &reset_field.leadingAnchor().constraintEqualToAnchor(&label_field.leadingAnchor()),
    &reset_field.trailingAnchor().constraintEqualToAnchor(&label_field.trailingAnchor()),
    // Progress bar: below label, pinned to sides.
    &progress.topAnchor().constraintEqualToAnchor_constant(&label_field.bottomAnchor(), 2.0),
    &progress.leadingAnchor().constraintEqualToAnchor_constant(&container.leadingAnchor(), H_PADDING),
    &progress.trailingAnchor().constraintEqualToAnchor_constant(&container.trailingAnchor(), -H_PADDING),
    &progress.heightAnchor().constraintEqualToConstant(H_PADDING),
    // Container bottom.
    &container.bottomAnchor().constraintEqualToAnchor_constant(&progress.bottomAnchor(), 2.0),
  ]);

  layout(&container);

  return container;
}

fn tier_badge_color(tier: &str) -> Retained<NSColor> {
  return match tier {
    "Free" => NSColor::colorWithSRGBRed_green_blue_alpha(0.60, 0.60, 0.60, 1.0),
    "Pro" => NSColor::colorWithSRGBRed_green_blue_alpha(0.30, 0.55, 0.90, 1.0),
    "Max 5x" => NSColor::colorWithSRGBRed_green_blue_alpha(0.55, 0.35, 0.85, 1.0),
    "Max 20x" => NSColor::colorWithSRGBRed_green_blue_alpha(0.85, 0.45, 0.20, 1.0),
    _ => NSColor::colorWithSRGBRed_green_blue_alpha(0.50, 0.50, 0.50, 1.0),
  };
}

pub fn header_row(mtm: MainThreadMarker, title: &str, tier: Option<&str>) -> Retained<NSView> {
  let container = NSView::init(mtm.alloc::<NSView>());

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
    &container.widthAnchor().constraintEqualToConstant(MENU_WIDTH),
    &field.leadingAnchor().constraintEqualToAnchor_constant(&container.leadingAnchor(), H_PADDING),
    &field.topAnchor().constraintEqualToAnchor_constant(&container.topAnchor(), 4.0),
    &container.bottomAnchor().constraintEqualToAnchor_constant(&field.bottomAnchor(), 2.0),
  ]);

  // Tier badge.
  if let Some(tier) = tier {
    let badge_font = NSFont::systemFontOfSize_weight(10.0, font_weight_medium());

    let badge_view = NSView::init(mtm.alloc::<NSView>());
    badge_view.noAutoresize();
    badge_view.setWantsLayer(true);

    container.addSubview(&badge_view);

    let badge_label = NSTextField::labelWithString(&NSString::from_str(tier), mtm);
    badge_label.noAutoresize();
    badge_label.setEditable(false);
    badge_label.setBezeled(false);
    badge_label.setDrawsBackground(false);
    badge_label.setFont(Some(&badge_font));
    badge_label.setTextColor(Some(&NSColor::whiteColor()));
    badge_label.setAlignment(objc2_app_kit::NSTextAlignment::Center);
    badge_view.addSubview(&badge_label);

    // Badge height is derived from the label's intrinsic height.
    activate(&[
      // Badge view: next to title, vertically centered.
      &badge_view.leadingAnchor().constraintEqualToAnchor_constant(&field.trailingAnchor(), 8.0),
      &badge_view.centerYAnchor().constraintEqualToAnchor(&field.centerYAnchor()),
      // Badge label fills badge view with padding; badge height wraps label.
      &badge_label.topAnchor().constraintEqualToAnchor_constant(&badge_view.topAnchor(), 1.0),
      &badge_label.bottomAnchor().constraintEqualToAnchor_constant(&badge_view.bottomAnchor(), -1.0),
      &badge_label.leadingAnchor().constraintEqualToAnchor_constant(&badge_view.leadingAnchor(), 6.0),
      &badge_label.trailingAnchor().constraintEqualToAnchor_constant(&badge_view.trailingAnchor(), -6.0),
    ]);

    // Round corners based on resolved height.
    badge_view.layoutSubtreeIfNeeded();
    if let Some(layer) = badge_view.layer() {
      let color = tier_badge_color(tier);
      layer.setBackgroundColor(Some(&color.CGColor()));
      layer.setCornerRadius(badge_view.fittingSize().height / 2.0);
    }
  }

  layout(&container);

  return container;
}

pub fn label_row(mtm: MainThreadMarker, text: &str, bold: bool) -> Retained<NSView> {
  let container = NSView::init(mtm.alloc::<NSView>());

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
    &container.widthAnchor().constraintEqualToConstant(MENU_WIDTH),
    &field.leadingAnchor().constraintEqualToAnchor_constant(&container.leadingAnchor(), H_PADDING),
    &field.trailingAnchor().constraintEqualToAnchor_constant(&container.trailingAnchor(), -H_PADDING),
    &field.topAnchor().constraintEqualToAnchor_constant(&container.topAnchor(), 3.0),
    &container.bottomAnchor().constraintEqualToAnchor_constant(&field.bottomAnchor(), 3.0),
  ]);

  layout(&container);

  return container;
}

pub fn key_value_row(mtm: MainThreadMarker, key: &str, value: &str) -> Retained<NSView> {
  let container = NSView::init(mtm.alloc::<NSView>());

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
    &container.widthAnchor().constraintEqualToConstant(MENU_WIDTH),
    &key_field.leadingAnchor().constraintEqualToAnchor_constant(&container.leadingAnchor(), H_PADDING),
    &key_field.topAnchor().constraintEqualToAnchor_constant(&container.topAnchor(), 3.0),
    &container.bottomAnchor().constraintEqualToAnchor_constant(&key_field.bottomAnchor(), 3.0),
    &value_field
      .trailingAnchor()
      .constraintEqualToAnchor_constant(&container.trailingAnchor(), -H_PADDING),
    &value_field.centerYAnchor().constraintEqualToAnchor(&key_field.centerYAnchor()),
  ]);

  layout(&container);

  return container;
}
