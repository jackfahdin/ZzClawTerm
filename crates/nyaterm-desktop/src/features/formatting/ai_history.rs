/// Tauri AI history date buckets (`groupSessionsByDate`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::features) enum AiHistoryDateGroup {
    Today,
    Yesterday,
    Last7Days,
    Earlier,
}

impl AiHistoryDateGroup {
    pub(in crate::features) fn label(self) -> &'static str {
        match self {
            Self::Today => "Today",
            Self::Yesterday => "Yesterday",
            Self::Last7Days => "Last 7 Days",
            Self::Earlier => "Earlier",
        }
    }
}

fn civil_day_number(year: i32, month: u32, day: u32) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = (y - era * 400) as i64;
    let mp = if month > 2 { month - 3 } else { month + 9 };
    let doy = ((153 * mp + 2) / 5 + day - 1) as i64;
    era as i64 * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy
}

fn utc_today_day_number() -> i64 {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0);
    // Unix epoch day 0 is 1970-01-01.
    719_468 + secs.div_euclid(86_400)
}

fn parse_rfc3339_day_number(value: &str) -> Option<i64> {
    let date = value.trim().get(..10)?;
    let mut parts = date.split('-');
    let year: i32 = parts.next()?.parse().ok()?;
    let month: u32 = parts.next()?.parse().ok()?;
    let day: u32 = parts.next()?.parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some(civil_day_number(year, month, day))
}

pub(in crate::features) fn ai_history_date_group(updated_at: &str) -> AiHistoryDateGroup {
    let today = utc_today_day_number();
    let Some(day) = parse_rfc3339_day_number(updated_at) else {
        return AiHistoryDateGroup::Earlier;
    };
    if day >= today {
        AiHistoryDateGroup::Today
    } else if day == today - 1 {
        AiHistoryDateGroup::Yesterday
    } else if day >= today - 6 {
        AiHistoryDateGroup::Last7Days
    } else {
        AiHistoryDateGroup::Earlier
    }
}

pub(in crate::features) fn group_ai_sessions_by_date(
    sessions: &[nyaterm_core::AiSession],
) -> [(AiHistoryDateGroup, Vec<nyaterm_core::AiSession>); 4] {
    let mut groups: [(AiHistoryDateGroup, Vec<nyaterm_core::AiSession>); 4] = [
        (AiHistoryDateGroup::Today, Vec::new()),
        (AiHistoryDateGroup::Yesterday, Vec::new()),
        (AiHistoryDateGroup::Last7Days, Vec::new()),
        (AiHistoryDateGroup::Earlier, Vec::new()),
    ];
    for session in sessions {
        let group = ai_history_date_group(&session.updated_at);
        let index = match group {
            AiHistoryDateGroup::Today => 0,
            AiHistoryDateGroup::Yesterday => 1,
            AiHistoryDateGroup::Last7Days => 2,
            AiHistoryDateGroup::Earlier => 3,
        };
        groups[index].1.push(session.clone());
    }
    groups
}
