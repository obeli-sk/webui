use gloo::timers::callback::Interval;
use jiff::{SignedDuration, Timestamp};
use yew::prelude::*;

pub fn relative_time(old: Timestamp, new: Timestamp, granularity: TimeGranularity) -> String {
    let duration = new.duration_since(old);
    human_formatted_timedelta(duration, granularity)
}

pub fn relative_time_if_significant(old: Timestamp, new: Timestamp) -> Option<String> {
    let duration = new.duration_since(old);
    if duration >= SignedDuration::from_secs(1) {
        Some(human_formatted_timedelta(
            new.duration_since(old),
            TimeGranularity::Coarse,
        ))
    } else {
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeGranularity {
    Fine,
    Coarse,
}

pub fn human_formatted_timedelta(duration: SignedDuration, granularity: TimeGranularity) -> String {
    if duration.as_secs() < 0 {
        return "in the future".to_string();
    }
    let millisecods = duration.as_millis();
    let seconds = duration.as_secs();
    let minutes = duration.as_mins();
    let hours = duration.as_hours();
    let days = duration.as_hours() / 24;

    match seconds {
        0 if granularity == TimeGranularity::Fine => {
            format!("{millisecods} ms")
        }
        0..=59 => {
            let plural = if seconds == 1 { "" } else { "s" };
            format!("{seconds} sec{plural}")
        }
        60..=3599 => {
            let plural = if minutes == 1 { "" } else { "s" };
            format!("{minutes} min{plural}")
        }
        3600..=86399 => {
            let plural = if hours == 1 { "" } else { "s" };
            format!("{hours} hour{plural}")
        }
        _ => {
            let plural = if days == 1 { "" } else { "s" };
            format!("{days} day{plural}")
        }
    }
}

/// Formats in the largest unit that divides the value exactly, for configured values that must not be rounded.
pub fn format_exact_millis(millis: u64) -> String {
    const UNITS: [(u64, &str, &str); 5] = [
        (24 * 60 * 60 * 1000, "day", "days"),
        (60 * 60 * 1000, "hour", "hours"),
        (60 * 1000, "min", "mins"),
        (1000, "sec", "secs"),
        (1, "ms", "ms"),
    ];
    let (factor, singular, plural) = UNITS
        .into_iter()
        .find(|(factor, ..)| millis != 0 && millis.is_multiple_of(*factor))
        .unwrap_or((1, "ms", "ms"));
    let value = millis / factor;
    format!("{value} {}", if value == 1 { singular } else { plural })
}

pub fn format_date(date: Timestamp) -> String {
    date.strftime("%Y-%m-%d %H:%M:%S%.3f").to_string()
}

/// Returns the current time and re-renders the calling component periodically so
/// that a label relative to `target` stays fresh without an external trigger.
///
/// The tick period adapts to how far away `target` is: [`TimeGranularity::Coarse`]
/// labels are stable within a minute (then an hour), so there's no need to tick
/// every second for a timestamp that's hours away.
#[hook]
pub fn use_relative_now(target: Timestamp) -> Timestamp {
    let trigger = use_force_update();
    let now = Timestamp::now();
    let secs = target.duration_since(now).as_secs().unsigned_abs();
    let period_ms: u32 = if secs < 60 {
        1_000
    } else if secs < 3_600 {
        30_000
    } else {
        300_000
    };
    use_effect_with(period_ms, move |&period_ms| {
        let interval = Interval::new(period_ms, move || trigger.force_update());
        move || drop(interval)
    });
    now
}

#[derive(Properties, PartialEq)]
pub struct RelativeAgoProps {
    pub target: Timestamp,
}

/// Self-updating "<duration> ago" label relative to the browser clock.
#[function_component(RelativeAgo)]
pub fn relative_ago(props: &RelativeAgoProps) -> Html {
    let now = use_relative_now(props.target);
    html! { {relative_ago_label(props.target, now)} }
}

fn relative_ago_label(target: Timestamp, now: Timestamp) -> String {
    if target > now {
        format!("in {}", relative_time(now, target, TimeGranularity::Coarse))
    } else {
        format!(
            "{} ago",
            relative_time(target, now, TimeGranularity::Coarse)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_labels_distinguish_past_and_future() {
        let now = "2026-10-02T12:00:00Z".parse::<Timestamp>().unwrap();
        assert_eq!(
            relative_ago_label(now - SignedDuration::from_mins(5), now),
            "5 mins ago"
        );
        assert_eq!(
            relative_ago_label(now + SignedDuration::from_mins(5), now),
            "in 5 mins"
        );
    }
}
