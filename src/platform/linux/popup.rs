use gtk4::prelude::*;
use gtk4::{self as gtk};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use crate::api::{ProfileResponse, SubscriptionTier, UsageBucket, UsageResponse};
use crate::util::format_reset_time;

const CSS: &str = r#"
window.popup {
  background-color: #1e1e1e;
}

.header-title {
  color: #ffffff;
  font-weight: 600;
  font-size: 14px;
}

.tier-badge {
  border-radius: 8px;
  padding: 1px 8px;
  font-size: 10px;
  font-weight: 500;
  color: #ffffff;
}
.tier-free   { background-color: #999999; }
.tier-pro    { background-color: #4D8CE6; }
.tier-max5x  { background-color: #8C59D9; }
.tier-max20x { background-color: #D97333; }

.bucket-label {
  color: #e0e0e0;
  font-size: 12px;
}
.bucket-reset {
  color: #888888;
  font-size: 10px;
  font-weight: 300;
}

.section-label {
  color: #e0e0e0;
  font-weight: 600;
  font-size: 12px;
}
.kv-key {
  color: #e0e0e0;
  font-size: 12px;
}
.kv-value {
  color: #888888;
  font-size: 12px;
}
.loading-label {
  color: #888888;
  font-size: 12px;
}

separator {
  background-color: #333333;
  min-height: 1px;
}

progressbar > trough {
  background-color: #333333;
  min-height: 8px;
  border-radius: 4px;
}
progressbar > trough > progress {
  min-height: 8px;
  border-radius: 4px;
}
progressbar.progress-normal > trough > progress { background-color: #cccccc; }
progressbar.progress-yellow > trough > progress { background-color: #ffcc00; }
progressbar.progress-orange > trough > progress { background-color: #ff9500; }
progressbar.progress-red > trough > progress    { background-color: #ff3b30; }

.action-button {
  background-color: #2a2a2a;
  color: #e0e0e0;
  border: 1px solid #444444;
  border-radius: 6px;
  padding: 6px 12px;
  font-size: 12px;
}
.action-button:hover {
  background-color: #383838;
}
"#;

pub struct PopupWidgets {
  pub window: gtk::Window,
  pub tier_badge: gtk::Label,
  pub buckets_box: gtk::Box,
  pub bucket_rows: Vec<BucketRow>,
  pub loading_label: gtk::Label,
  pub extra_separator: gtk::Separator,
  pub extra_box: gtk::Box,
  pub extra_value: gtk::Label,
  pub refresh_btn: gtk::Button,
}

pub struct BucketRow {
  container: gtk::Box,
  label: gtk::Label,
  reset: gtk::Label,
  progress: gtk::ProgressBar,
}

pub fn build_popup(app: &gtk::Application) -> PopupWidgets {
  let provider = gtk::CssProvider::new();
  provider.load_from_data(CSS);
  gtk::style_context_add_provider_for_display(
    &gtk::gdk::Display::default().expect("no display"),
    &provider,
    gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
  );

  let window = gtk::Window::builder()
    .application(app)
    .title("Claude Usage")
    .decorated(false)
    .resizable(false)
    .default_width(280)
    .build();
  window.add_css_class("popup");

  if gtk4_layer_shell::is_supported() {
    window.init_layer_shell();
    window.set_layer(Layer::Overlay);
    window.set_keyboard_mode(KeyboardMode::OnDemand);
  }

  let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
  vbox.set_margin_start(14);
  vbox.set_margin_end(14);
  vbox.set_margin_top(10);
  vbox.set_margin_bottom(10);

  // Header
  let header_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
  header_box.set_margin_bottom(6);

  let title = gtk::Label::new(Some("Claude Usage"));
  title.add_css_class("header-title");
  title.set_halign(gtk::Align::Start);
  header_box.append(&title);

  let tier_badge = gtk::Label::new(None);
  tier_badge.add_css_class("tier-badge");
  tier_badge.set_visible(false);
  tier_badge.set_valign(gtk::Align::Center);
  header_box.append(&tier_badge);

  vbox.append(&header_box);
  vbox.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

  // Loading label
  let loading_label = gtk::Label::new(Some("Loading..."));
  loading_label.add_css_class("loading-label");
  loading_label.set_halign(gtk::Align::Start);
  loading_label.set_margin_top(6);
  loading_label.set_margin_bottom(6);
  vbox.append(&loading_label);

  // Buckets container
  let buckets_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
  buckets_box.set_visible(false);
  let bucket_rows = create_bucket_rows(&buckets_box);
  vbox.append(&buckets_box);

  // Extra usage
  let extra_separator = gtk::Separator::new(gtk::Orientation::Horizontal);
  extra_separator.set_visible(false);
  vbox.append(&extra_separator);

  let extra_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
  extra_box.set_visible(false);
  extra_box.set_margin_top(6);

  let extra_header = gtk::Label::new(Some("Extra Usage"));
  extra_header.add_css_class("section-label");
  extra_header.set_halign(gtk::Align::Start);
  extra_box.append(&extra_header);

  let spent_row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
  let spent_key = gtk::Label::new(Some("Spent"));
  spent_key.add_css_class("kv-key");
  spent_key.set_halign(gtk::Align::Start);
  spent_key.set_hexpand(true);
  spent_row.append(&spent_key);

  let extra_value = gtk::Label::new(None);
  extra_value.add_css_class("kv-value");
  extra_value.set_halign(gtk::Align::End);
  spent_row.append(&extra_value);

  extra_box.append(&spent_row);
  vbox.append(&extra_box);

  // Separator before buttons
  let btn_sep = gtk::Separator::new(gtk::Orientation::Horizontal);
  btn_sep.set_margin_top(6);
  vbox.append(&btn_sep);

  // Buttons
  let btn_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
  btn_box.set_homogeneous(true);
  btn_box.set_margin_top(8);

  let refresh_btn = gtk::Button::with_label("Refresh");
  refresh_btn.add_css_class("action-button");
  btn_box.append(&refresh_btn);

  let quit_btn = gtk::Button::with_label("Quit");
  quit_btn.add_css_class("action-button");
  quit_btn.connect_clicked(|_| std::process::exit(0));
  btn_box.append(&quit_btn);

  vbox.append(&btn_box);
  window.set_child(Some(&vbox));

  // Hide on focus loss
  window.connect_is_active_notify(|win| {
    if !win.is_active() {
      win.set_visible(false);
    }
  });

  return PopupWidgets {
    window,
    tier_badge,
    buckets_box,
    bucket_rows,
    loading_label,
    extra_separator,
    extra_box,
    extra_value,
    refresh_btn,
  };
}

fn create_bucket_rows(parent: &gtk::Box) -> Vec<BucketRow> {
  return ["5h Limit", "7d Limit", "7d Sonnet", "7d Opus"]
    .iter()
    .map(|name| {
      let container = gtk::Box::new(gtk::Orientation::Vertical, 2);
      container.set_margin_top(6);
      container.set_visible(false);

      let top_row = gtk::Box::new(gtk::Orientation::Horizontal, 0);

      let label = gtk::Label::new(Some(&format!("{}  0%", name)));
      label.add_css_class("bucket-label");
      label.set_halign(gtk::Align::Start);
      label.set_hexpand(true);
      top_row.append(&label);

      let reset = gtk::Label::new(None);
      reset.add_css_class("bucket-reset");
      reset.set_halign(gtk::Align::End);
      top_row.append(&reset);

      container.append(&top_row);

      let progress = gtk::ProgressBar::new();
      progress.set_fraction(0.0);
      progress.add_css_class("progress-normal");
      container.append(&progress);

      parent.append(&container);

      BucketRow { container, label, reset, progress }
    })
    .collect();
}

fn progress_color_class(utilization: f64) -> &'static str {
  match utilization {
    u if u < 50.0 => "progress-normal",
    u if u < 75.0 => "progress-yellow",
    u if u < 90.0 => "progress-orange",
    _ => "progress-red",
  }
}

fn tier_css_class(tier: SubscriptionTier) -> &'static str {
  match tier {
    SubscriptionTier::Free => "tier-free",
    SubscriptionTier::Pro => "tier-pro",
    SubscriptionTier::Max5x => "tier-max5x",
    SubscriptionTier::Max20x => "tier-max20x",
  }
}

pub fn update_popup(
  widgets: &PopupWidgets,
  usage: &UsageResponse,
  profile: &Option<ProfileResponse>,
) {
  widgets.loading_label.set_visible(false);
  widgets.buckets_box.set_visible(true);

  if let Some(p) = profile {
    let tier = p.organization.rate_limit_tier;
    widgets.tier_badge.set_text(&tier.to_string());
    for cls in ["tier-free", "tier-pro", "tier-max5x", "tier-max20x"] {
      widgets.tier_badge.remove_css_class(cls);
    }
    widgets.tier_badge.add_css_class(tier_css_class(tier));
    widgets.tier_badge.set_visible(true);
  }

  let buckets: [(&str, &Option<UsageBucket>); 4] = [
    ("5h Limit", &usage.five_hour),
    ("7d Limit", &usage.seven_day),
    ("7d Sonnet", &usage.seven_day_sonnet),
    ("7d Opus", &usage.seven_day_opus),
  ];

  for (i, (name, bucket_opt)) in buckets.iter().enumerate() {
    let row = &widgets.bucket_rows[i];
    if let Some(bucket) = bucket_opt {
      row.container.set_visible(true);
      row.label.set_text(&format!("{}  {}%", name, bucket.utilization as u32));
      row.reset.set_text(&format!("resets in {}", format_reset_time(&bucket.resets_at)));
      row.progress.set_fraction(bucket.utilization / 100.0);
      for cls in ["progress-normal", "progress-yellow", "progress-orange", "progress-red"] {
        row.progress.remove_css_class(cls);
      }
      row.progress.add_css_class(progress_color_class(bucket.utilization));
    } else {
      row.container.set_visible(false);
    }
  }

  if let Some(extra) = &usage.extra_usage {
    if extra.is_enabled {
      widgets.extra_separator.set_visible(true);
      widgets.extra_box.set_visible(true);
      let limit = extra.monthly_limit / 100.0;
      let used = extra.used_credits / 100.0;
      widgets.extra_value.set_text(&format!("${:.2} / ${:.2}", used, limit));
      return;
    }
  }
  widgets.extra_separator.set_visible(false);
  widgets.extra_box.set_visible(false);
}

pub fn toggle_popup(widgets: &PopupWidgets, icon_x: i32, icon_y: i32) {
  let win = &widgets.window;
  if win.is_visible() {
    win.set_visible(false);
    return;
  }

  if gtk4_layer_shell::is_supported() {
    if let Some(display) = gtk::gdk::Display::default() {
      let popup_width = win.default_width();

      // Find the monitor that contains the icon coordinates
      let monitors = display.monitors();
      let monitor = (0..monitors.n_items())
        .filter_map(|i| monitors.item(i)?.downcast::<gtk::gdk::Monitor>().ok())
        .find(|m| {
          let g = m.geometry();
          icon_x >= g.x() && icon_x < g.x() + g.width() && icon_y >= g.y() && icon_y < g.y() + g.height()
        })
        .or_else(|| monitors.item(0)?.downcast::<gtk::gdk::Monitor>().ok());

      if let Some(monitor) = monitor {
        let geom = monitor.geometry();
        win.set_monitor(Some(&monitor));

        // Convert global coords to monitor-relative
        let rel_x = icon_x - geom.x();
        let rel_y = icon_y - geom.y();

        // Center horizontally over the icon
        let left = (rel_x - popup_width / 2).clamp(8, geom.width() - popup_width - 8);
        win.set_anchor(Edge::Left, true);
        win.set_anchor(Edge::Right, false);
        win.set_margin(Edge::Left, left);

        // Place above or below the icon depending on panel position
        if rel_y > geom.height() / 2 {
          win.set_anchor(Edge::Bottom, true);
          win.set_anchor(Edge::Top, false);
          win.set_margin(Edge::Bottom, geom.height() - rel_y + 8);
        } else {
          win.set_anchor(Edge::Top, true);
          win.set_anchor(Edge::Bottom, false);
          win.set_margin(Edge::Top, rel_y + 8);
        }
      }
    }
  }

  win.present();
}
