use jiff::Timestamp;

pub fn format_reset_time(resets_at: &Timestamp) -> String {
  let now = Timestamp::now();
  let diff = resets_at.as_second() - now.as_second();

  if diff <= 0 {
    return "now".to_string();
  }

  let days = diff / 86400;
  let hours = (diff % 86400) / 3600;
  let mins = (diff % 3600) / 60;

  if days > 0 {
    return format!("{}d {}h", days, hours);
  }

  if hours > 0 {
    return format!("{}h {}m", hours, mins);
  }

  return format!("{}m", mins);
}

pub fn format_absolute_time(resets_at: &Timestamp) -> String {
  let dt = resets_at.to_zoned(jiff::tz::TimeZone::system());
  return format!("{:02}.{:02}, {:02}:{:02}", dt.day(), dt.month(), dt.hour(), dt.minute());
}

/// Formats a future timestamp as "HH:MM", "tomorrow, HH:MM", or "DD.MM, HH:MM".
pub fn format_until_time(ts: &Timestamp) -> String {
  let tz = jiff::tz::TimeZone::system();
  let target = ts.to_zoned(tz.clone());
  let now = Timestamp::now().to_zoned(tz);

  let time = format!("{:02}:{:02}", target.hour(), target.minute());

  if target.date() == now.date() {
    return time;
  }

  let one_day = jiff::SignedDuration::from_hours(24);
  if let Ok(tomorrow) = now.date().checked_add(one_day) {
    let target_date = target.date();

    if target_date == tomorrow {
      return format!("tomorrow, {}", time);
    }
  }

  return format!("{:02}.{:02}, {}", target.day(), target.month(), time);
}
