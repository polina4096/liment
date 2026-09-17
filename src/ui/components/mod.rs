//! Custom `NSView`s used as menu rows, plus the small layout primitives they share.

use objc2::{MainThreadMarker, Message, rc::Retained};
use objc2_app_kit::{NSFont, NSLayoutConstraint, NSTextAlignment, NSTextField, NSView};
use objc2_core_foundation::CGFloat;
use objc2_foundation::{NSArray, NSString};

use crate::utils::macos::NSViewExt;

mod bucket;
mod header;
mod peak_hours;
mod text;

pub use bucket::{BucketRowParams, bucket_row};
pub use header::header_row;
pub use peak_hours::peak_hours_row;
pub use text::{key_value_row, label_row};

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

/// The small, light, secondary-colored font used for right-aligned detail text.
fn detail_font() -> Retained<NSFont> {
  return NSFont::systemFontOfSize_weight(10.0, font_weight_light());
}

fn activate(constraints: &[&NSLayoutConstraint]) {
  let array = NSArray::from_retained_slice(&constraints.iter().map(|c| c.retain()).collect::<Vec<_>>());

  return NSLayoutConstraint::activateConstraints(&array);
}

/// A non-editable, borderless, transparent text field ready for Auto Layout.
fn make_label(mtm: MainThreadMarker, text: &str, font: &NSFont) -> Retained<NSTextField> {
  let field = NSTextField::labelWithString(&NSString::from_str(text), mtm);
  field.noAutoresize();
  field.setEditable(false);
  field.setBezeled(false);
  field.setDrawsBackground(false);
  field.setFont(Some(font));

  return field;
}

/// Same as [`make_label`] but right-aligned, for trailing detail text.
fn make_right_label(mtm: MainThreadMarker, text: &str, font: &NSFont) -> Retained<NSTextField> {
  let field = make_label(mtm, text, font);
  field.setAlignment(NSTextAlignment::Right);

  return field;
}

/// A fixed-width menu row container.
fn make_container(mtm: MainThreadMarker) -> Retained<NSView> {
  let container = NSView::init(mtm.alloc::<NSView>());
  activate(&[&container.widthAnchor().constraintEqualToConstant(MENU_WIDTH)]);

  return container;
}

/// Resolves Auto Layout constraints and updates the container's frame.
fn layout(container: &NSView) {
  container.layoutSubtreeIfNeeded();
  container.setFrameSize(container.fittingSize());
}
