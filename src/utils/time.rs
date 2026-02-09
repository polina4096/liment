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
