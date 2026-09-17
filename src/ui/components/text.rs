//! Plain text rows: a single label, or a key on the left with a value on the right.

use objc2::{MainThreadMarker, rc::Retained};
use objc2_app_kit::{NSColor, NSFont, NSView};

use super::{
  H_PADDING, activate, font_weight_regular, font_weight_semibold, layout, make_container, make_label, make_right_label,
};

pub fn label_row(mtm: MainThreadMarker, text: &str, bold: bool) -> Retained<NSView> {
  let container = make_container(mtm);

  let weight = if bold { font_weight_semibold() } else { font_weight_regular() };
  let field = make_label(mtm, text, &NSFont::systemFontOfSize_weight(12.0, weight));
  container.addSubview(&field);

  activate(&[
    &field.leadingAnchor().constraintEqualToAnchor_constant(&container.leadingAnchor(), H_PADDING),
    &field.trailingAnchor().constraintEqualToAnchor_constant(&container.trailingAnchor(), -H_PADDING),
    &field.topAnchor().constraintEqualToAnchor_constant(&container.topAnchor(), 3.0),
    &container.bottomAnchor().constraintEqualToAnchor_constant(&field.bottomAnchor(), 3.0),
  ]);

  layout(&container);

  return container;
}

pub fn key_value_row(mtm: MainThreadMarker, key: &str, value: &str) -> Retained<NSView> {
  let container = make_container(mtm);
  let font = NSFont::systemFontOfSize_weight(12.0, font_weight_regular());

  let key_field = make_label(mtm, key, &font);
  container.addSubview(&key_field);

  let value_field = make_right_label(mtm, value, &font);
  value_field.setTextColor(Some(&NSColor::secondaryLabelColor()));
  container.addSubview(&value_field);

  activate(&[
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
