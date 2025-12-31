use chrono::{DateTime, TimeDelta, Utc};

pub fn relative_time(old: DateTime<Utc>, new: DateTime<Utc>) -> String {
    human_formatted_timedelta(new.signed_duration_since(old), TimeGranularity::Coarse)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeGranularity {
    Fine,
    Coarse,
}

pub fn human_formatted_timedelta(duration: TimeDelta, granularity: TimeGranularity) -> String {
    if duration.num_seconds() < 0 {
        return "in the future".to_string();
    }
    let millisecods = duration.num_milliseconds();
    let seconds = duration.num_seconds();
    let minutes = duration.num_minutes();
    let hours = duration.num_hours();
    let days = duration.num_days();

    match seconds {
        0 if granularity == TimeGranularity::Fine => {
            format!("{millisecods} ms")
        }
        0..=59 if granularity == TimeGranularity::Fine => {
            let plural = if seconds == 1 { "" } else { "s" };
            format!("{seconds} sec{plural}")
        }
        0..=59 => "just now".to_string(),
        60..=3599 => {
            let plural = if minutes == 1 { "" } else { "s" };
            format!("{minutes} min{plural}")
        }
        3600..=86399 => {
            let plural = if hours == 1 { "" } else { "s" };
            format!("{hours} hour{plural}")
        }
        _ => {
            if days < 30 {
                let plural = if days == 1 { "" } else { "s" };
                format!("{days} day{plural}")
            } else if days < 365 {
                let months = days / 30;
                let plural = if months == 1 { "" } else { "s" };
                format!("{months} month{plural}")
            } else {
                let years = days / 365;
                let plural = if years == 1 { "" } else { "s" };
                format!("{years} year{plural}")
            }
        }
    }
}
