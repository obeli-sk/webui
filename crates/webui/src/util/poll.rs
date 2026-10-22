/// Delay bounds for polling an execution's status.
pub const STATUS_POLL_MS: (u32, u32) = (1_000, 5_000);
/// Delay bounds for polling an execution's events and responses.
pub const HISTORY_POLL_MS: (u32, u32) = (2_500, 10_000);

/// Doubles the previous delay while nothing changes, resets to the minimum once something does.
pub fn next_poll_delay(prev_ms: u32, changed: bool, (min_ms, max_ms): (u32, u32)) -> u32 {
    if changed {
        min_ms
    } else {
        prev_ms.saturating_mul(2).clamp(min_ms, max_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backs_off_until_change() {
        let bounds = (1_000, 5_000);
        let delays: Vec<_> = [false, false, false, false, true, false]
            .into_iter()
            .scan(0, |prev, changed| {
                *prev = next_poll_delay(*prev, changed, bounds);
                Some(*prev)
            })
            .collect();
        assert_eq!(delays, [1_000, 2_000, 4_000, 5_000, 1_000, 2_000]);
    }
}
