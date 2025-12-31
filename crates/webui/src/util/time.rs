use chrono::{DateTime, Utc};

pub fn relative_time(old: DateTime<Utc>, new: DateTime<Utc>) -> String {
    let duration = new.signed_duration_since(old);

    if duration.num_seconds() < 0 {
        return "in the future".to_string();
    }

    let seconds = duration.num_seconds();
    let minutes = duration.num_minutes();
    let hours = duration.num_hours();
    let days = duration.num_days();

    match seconds {
        0..=59 => "just now".to_string(),
        60..=3599 => {
            let s = if minutes == 1 { "" } else { "s" };
            format!("{} min{} ago", minutes, s)
        }
        3600..=86399 => {
            let s = if hours == 1 { "" } else { "s" };
            format!("{} hour{} ago", hours, s)
        }
        _ => {
            if days < 30 {
                let s = if days == 1 { "" } else { "s" };
                format!("{} day{} ago", days, s)
            } else if days < 365 {
                let months = days / 30;
                let s = if months == 1 { "" } else { "s" };
                format!("{} month{} ago", months, s)
            } else {
                let years = days / 365;
                let s = if years == 1 { "" } else { "s" };
                format!("{} year{} ago", years, s)
            }
        }
    }
}
