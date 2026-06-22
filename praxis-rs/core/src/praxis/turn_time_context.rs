use chrono::Local;
use chrono::Utc;

pub(super) fn local_time_context() -> (String, String) {
    match iana_time_zone::get_timezone() {
        Ok(timezone) => (Local::now().format("%Y-%m-%d").to_string(), timezone),
        Err(_) => (
            Utc::now().format("%Y-%m-%d").to_string(),
            "Etc/UTC".to_string(),
        ),
    }
}
