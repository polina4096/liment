//! The menu header: title, optional tier badge, optional version.

use objc2::{MainThreadMarker, rc::Retained};
use objc2_app_kit::{NSColor, NSFont, NSTextAlignment, NSView};

use super::{
  H_PADDING, activate, detail_font, font_weight_medium, font_weight_semibold, layout, make_container, make_label,
  make_right_label,
};
use crate::{providers::Tier, utils::macos::NSViewExt};

pub fn header_row(mtm: MainThreadMarker, title: &str, tier: &Option<&Tier>, version: Option<&str>) -> Retained<NSView> {
  let container = make_container(mtm);

  let title_field = make_label(mtm, title, &NSFont::systemFontOfSize_weight(14.0, font_weight_semibold()));
  container.addSubview(&title_field);

  activate(&[
    &title_field.leadingAnchor().constraintEqualToAnchor_constant(&container.leadingAnchor(), H_PADDING),
    &title_field.topAnchor().constraintEqualToAnchor_constant(&container.topAnchor(), 4.0),
    &container.bottomAnchor().constraintEqualToAnchor_constant(&title_field.bottomAnchor(), 2.0),
  ]);

  if let Some(version) = version {
    let version_field = make_right_label(mtm, version, &detail_font());
    version_field.setTextColor(Some(&NSColor::tertiaryLabelColor()));
    container.addSubview(&version_field);

    activate(&[
      &version_field
        .trailingAnchor()
        .constraintEqualToAnchor_constant(&container.trailingAnchor(), -H_PADDING),
      &version_field.centerYAnchor().constraintEqualToAnchor(&title_field.centerYAnchor()),
    ]);
  }

  if let Some(tier) = tier {
    let badge = tier_badge(mtm, tier);
    container.addSubview(&badge);

    activate(&[
      &badge.leadingAnchor().constraintEqualToAnchor_constant(&title_field.trailingAnchor(), 8.0),
      &badge.centerYAnchor().constraintEqualToAnchor(&title_field.centerYAnchor()),
    ]);

    round_badge(&badge, tier);
  }

  layout(&container);

  return container;
}

/// A colored pill with the tier name in white.
fn tier_badge(mtm: MainThreadMarker, tier: &Tier) -> Retained<NSView> {
  let badge = NSView::init(mtm.alloc::<NSView>());
  badge.noAutoresize();
  badge.setWantsLayer(true);

  let label = make_label(mtm, &tier.name, &NSFont::systemFontOfSize_weight(10.0, font_weight_medium()));
  label.setTextColor(Some(&NSColor::whiteColor()));
  label.setAlignment(NSTextAlignment::Center);
  badge.addSubview(&label);

  // The badge wraps the label with padding; its height follows the label's intrinsic height.
  activate(&[
    &label.topAnchor().constraintEqualToAnchor_constant(&badge.topAnchor(), 1.0),
    &label.bottomAnchor().constraintEqualToAnchor_constant(&badge.bottomAnchor(), -1.0),
    &label.leadingAnchor().constraintEqualToAnchor_constant(&badge.leadingAnchor(), 6.0),
    &label.trailingAnchor().constraintEqualToAnchor_constant(&badge.trailingAnchor(), -6.0),
  ]);

  return badge;
}

/// Colors the badge and rounds it into a pill. Needs the badge to be in a view hierarchy
/// with its constraints active, since the corner radius comes from the resolved height.
fn round_badge(badge: &NSView, tier: &Tier) {
  badge.layoutSubtreeIfNeeded();

  let Some(layer) = badge.layer()
  else {
    return;
  };

  let r = f64::from(tier.color.r) / 255.0;
  let g = f64::from(tier.color.g) / 255.0;
  let b = f64::from(tier.color.b) / 255.0;
  let color = NSColor::colorWithSRGBRed_green_blue_alpha(r, g, b, 1.0);

  layer.setBackgroundColor(Some(&color.CGColor()));
  layer.setCornerRadius(badge.fittingSize().height / 2.0);
}
