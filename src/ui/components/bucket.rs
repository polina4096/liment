//! A usage-window row: "5h Limit  42%" with a reset time and a progress bar.

use jiff::Timestamp;
use objc2::{MainThreadMarker, rc::Retained};
use objc2_app_kit::{
  NSColor, NSFont, NSLayoutConstraintOrientation, NSLayoutPriorityDefaultLow, NSLineBreakMode, NSMenuItem,
  NSProgressIndicator, NSProgressIndicatorStyle, NSView,
};
use objc2_foundation::NSString;

use super::{
  H_PADDING, activate, detail_font, font_weight_regular, layout, make_container, make_label, make_right_label,
};
use crate::{
  config::{DateTimeFormat, DisplayMode},
  utils::{
    macos::NSViewExt,
    time::{format_absolute_time, format_reset_time},
  },
};

pub struct BucketRowParams<'a> {
  pub label: &'a str,
  pub utilization: f64,
  pub resets_at: Option<&'a Timestamp>,
  pub period_seconds: Option<i64>,
  pub show_period_percentage: bool,
  pub show_pacing_warning: bool,
  pub reset_time_format: DateTimeFormat,
  pub display_mode: DisplayMode,
}

/// The right-aligned detail text of a bucket row, e.g. "resets in 3h 12m (64%) ⚠".
struct ResetText {
  text: String,
  pacing_warning: bool,
}

pub fn bucket_row(mtm: MainThreadMarker, params: &BucketRowParams) -> Retained<NSMenuItem> {
  let reset = params.resets_at.map(|resets_at| reset_text(params, resets_at));

  let utilization = match params.display_mode {
    DisplayMode::Remaining => 100.0 - params.utilization,
    DisplayMode::Usage => params.utilization,
  };
  let pacing_warning = reset.as_ref().is_some_and(|r| r.pacing_warning);
  let reset_color = if pacing_warning { Some(NSColor::systemYellowColor()) } else { None };

  let view =
    progress_row(mtm, params.label, utilization, reset.as_ref().map(|r| r.text.as_str()), reset_color.as_deref());
  let item = NSMenuItem::new(mtm);
  item.setView(Some(&view));

  return item;
}

/// Formats the reset time, optionally followed by the elapsed period percentage and a
/// pacing warning glyph when utilization is ahead of elapsed time.
fn reset_text(params: &BucketRowParams, resets_at: &Timestamp) -> ResetText {
  let mut text = match params.reset_time_format {
    DateTimeFormat::Absolute => format!("reset: {}", format_absolute_time(resets_at)),
    DateTimeFormat::Relative => format!("resets in {}", format_reset_time(resets_at)),
  };
  let mut pacing_warning = false;

  if (params.show_period_percentage || params.show_pacing_warning)
    && let Some(elapsed_pct) = elapsed_pct(params.period_seconds, resets_at)
  {
    if params.show_period_percentage {
      let display_pct = match params.display_mode {
        DisplayMode::Remaining => 100.0 - elapsed_pct,
        DisplayMode::Usage => elapsed_pct,
      };
      text = format!("{} ({:.0}%)", text, display_pct);
    }

    if params.show_pacing_warning && params.utilization > elapsed_pct {
      text = format!("{} ⚠", text);
      pacing_warning = true;
    }
  }

  return ResetText { text, pacing_warning };
}

/// How much of the bucket's period has elapsed, in percent. `None` if the period is unknown
/// or already over.
fn elapsed_pct(period_seconds: Option<i64>, resets_at: &Timestamp) -> Option<f64> {
  let period = period_seconds?;
  let remaining = resets_at.as_second() - Timestamp::now().as_second();

  if remaining <= 0 || period <= 0 {
    return None;
  }

  let passed = period - remaining;

  return Some((passed as f64 / period as f64 * 100.0).clamp(0.0, 100.0));
}

fn progress_row(
  mtm: MainThreadMarker,
  label: &str,
  utilization: f64,
  reset_str: Option<&str>,
  reset_color: Option<&NSColor>,
) -> Retained<NSView> {
  let container = make_container(mtm);
  let font = NSFont::systemFontOfSize_weight(12.0, font_weight_regular());
  let percent = format!("{}%", utilization as i64);

  // Label: "5h Limit", truncated with an ellipsis when the row runs out of space.
  let label_field = make_label(mtm, label, &font);
  label_field.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
  label_field.setContentCompressionResistancePriority_forOrientation(
    NSLayoutPriorityDefaultLow,
    NSLayoutConstraintOrientation::Horizontal,
  );
  container.addSubview(&label_field);

  // Utilization percentage, always kept intact next to the label.
  let value_field = make_label(mtm, &percent, &font);
  container.addSubview(&value_field);

  // Tooltip with the untruncated row text, set on every hovered subview since tooltips
  // are not inherited from the superview.
  let tooltip = match reset_str {
    Some(reset_str) => format!("{}  {} · {}", label, percent, reset_str),
    None => format!("{}  {}", label, percent),
  };
  let tooltip = NSString::from_str(&tooltip);
  container.setToolTip(Some(&tooltip));
  label_field.setToolTip(Some(&tooltip));
  value_field.setToolTip(Some(&tooltip));

  activate(&[
    &value_field.firstBaselineAnchor().constraintEqualToAnchor(&label_field.firstBaselineAnchor()),
    &value_field.leadingAnchor().constraintEqualToAnchor_constant(&label_field.trailingAnchor(), 6.0),
    &value_field
      .trailingAnchor()
      .constraintLessThanOrEqualToAnchor_constant(&container.trailingAnchor(), -H_PADDING),
  ]);

  // Reset time label (right-aligned), only if reset info is available.
  if let Some(reset_str) = reset_str {
    let reset_field = make_right_label(mtm, reset_str, &detail_font());
    let default_color = NSColor::secondaryLabelColor();
    reset_field.setTextColor(Some(reset_color.unwrap_or(&default_color)));
    reset_field.setToolTip(Some(&tooltip));
    container.addSubview(&reset_field);

    activate(&[
      // Same row as the label, right-aligned after the percentage.
      &reset_field.topAnchor().constraintEqualToAnchor(&label_field.topAnchor()),
      &reset_field
        .leadingAnchor()
        .constraintGreaterThanOrEqualToAnchor_constant(&value_field.trailingAnchor(), 8.0),
      &reset_field
        .trailingAnchor()
        .constraintEqualToAnchor_constant(&container.trailingAnchor(), -H_PADDING),
    ]);
  }

  let progress = progress_bar(mtm, utilization);
  container.addSubview(&progress);

  activate(&[
    // Label row: top, leading.
    &label_field.topAnchor().constraintEqualToAnchor_constant(&container.topAnchor(), 6.0),
    &label_field.leadingAnchor().constraintEqualToAnchor_constant(&container.leadingAnchor(), H_PADDING),
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

fn progress_bar(mtm: MainThreadMarker, value: f64) -> Retained<NSProgressIndicator> {
  let progress = NSProgressIndicator::init(mtm.alloc::<NSProgressIndicator>());
  progress.noAutoresize();
  progress.setStyle(NSProgressIndicatorStyle::Bar);
  progress.setIndeterminate(false);
  progress.setMinValue(0.0);
  progress.setMaxValue(100.0);
  progress.setDoubleValue(value);

  return progress;
}
