//! The "Peak hours / Off-peak · until HH:MM" indicator row.

use objc2::{MainThreadMarker, rc::Retained};
use objc2_app_kit::{NSColor, NSFont, NSView};
use objc2_core_foundation::CGFloat;

use super::{
  H_PADDING, activate, detail_font, font_weight_medium, font_weight_regular, layout, make_container, make_label,
  make_right_label,
};
use crate::{
  providers::PeakHours,
  utils::{macos::NSViewExt, time::format_until_time},
};

const DOT_SIZE: CGFloat = 7.0;

pub fn peak_hours_row(mtm: MainThreadMarker, info: &PeakHours) -> Retained<NSView> {
  let container = make_container(mtm);
  let accent_color = if info.is_peak { NSColor::systemOrangeColor() } else { NSColor::secondaryLabelColor() };

  let dot = status_dot(mtm, &accent_color);
  container.addSubview(&dot);

  let weight = if info.is_peak { font_weight_medium() } else { font_weight_regular() };
  let label_text = if info.is_peak { "Peak hours" } else { "Off-peak" };
  let label_field = make_label(mtm, label_text, &NSFont::systemFontOfSize_weight(11.0, weight));
  label_field.setTextColor(Some(&accent_color));
  container.addSubview(&label_field);

  let time_field = make_right_label(mtm, &format!("until {}", format_until_time(&info.ends_at)), &detail_font());
  time_field.setTextColor(Some(&NSColor::secondaryLabelColor()));
  container.addSubview(&time_field);

  activate(&[
    // Dot: leading, centered on the label row.
    &dot.leadingAnchor().constraintEqualToAnchor_constant(&container.leadingAnchor(), H_PADDING),
    &dot.centerYAnchor().constraintEqualToAnchor(&label_field.centerYAnchor()),
    // Label: right after the dot.
    &label_field.leadingAnchor().constraintEqualToAnchor_constant(&dot.trailingAnchor(), 6.0),
    &label_field.topAnchor().constraintEqualToAnchor_constant(&container.topAnchor(), 2.0),
    // Time: right-aligned, centered with the label.
    &time_field
      .trailingAnchor()
      .constraintEqualToAnchor_constant(&container.trailingAnchor(), -H_PADDING),
    &time_field.centerYAnchor().constraintEqualToAnchor(&label_field.centerYAnchor()),
    // Container bottom.
    &container.bottomAnchor().constraintEqualToAnchor_constant(&label_field.bottomAnchor(), 3.0),
  ]);

  layout(&container);

  return container;
}

/// A small filled circle.
fn status_dot(mtm: MainThreadMarker, color: &NSColor) -> Retained<NSView> {
  let dot = NSView::init(mtm.alloc::<NSView>());
  dot.noAutoresize();
  dot.setWantsLayer(true);

  if let Some(layer) = dot.layer() {
    layer.setBackgroundColor(Some(&color.CGColor()));
    layer.setCornerRadius(DOT_SIZE / 2.0);
  }

  activate(&[
    &dot.widthAnchor().constraintEqualToConstant(DOT_SIZE),
    &dot.heightAnchor().constraintEqualToConstant(DOT_SIZE),
  ]);

  return dot;
}
