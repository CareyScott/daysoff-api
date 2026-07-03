use chrono::{Datelike, NaiveDate, Weekday};

pub fn is_weekend(d: NaiveDate) -> bool {
    matches!(d.weekday(), Weekday::Sat | Weekday::Sun)
}

/// Count Mon-Fri days in [start, end] inclusive.
pub fn business_days(start: NaiveDate, end: NaiveDate) -> i32 {
    let mut d = start;
    let mut n = 0;
    while d <= end {
        if !is_weekend(d) {
            n += 1;
        }
        d = d.succ_opt().expect("date overflow");
    }
    n
}

pub fn current_year() -> i32 {
    chrono::Utc::now().date_naive().year()
}
