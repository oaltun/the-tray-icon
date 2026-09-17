#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Status {
    Ok,
    Warn,
    Err,
}

impl Status {
    pub fn as_str(&self) -> &'static str {
        match self {
            Status::Ok => "ok",
            Status::Warn => "warn",
            Status::Err => "err",
        }
    }

    pub fn glyph(&self) -> &'static str {
        match self {
            Status::Ok => "✓",
            Status::Warn => "⚠",
            Status::Err => "✗",
        }
    }

    /// Parses a script-output header line like "# ok", "# warn", "# err".
    /// Matching is case-insensitive and ignores surrounding whitespace.
    pub fn from_header(line: &str) -> Option<Status> {
        match line.trim().to_lowercase().as_str() {
            "# ok" => Some(Status::Ok),
            "# warn" => Some(Status::Warn),
            "# err" => Some(Status::Err),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_parsing() {
        assert_eq!(Status::from_header("# ok"), Some(Status::Ok));
        assert_eq!(Status::from_header("# warn"), Some(Status::Warn));
        assert_eq!(Status::from_header("# err"), Some(Status::Err));
        assert_eq!(Status::from_header("  # OK  "), Some(Status::Ok));
        assert_eq!(Status::from_header("# nope"), None);
        assert_eq!(Status::from_header("ok"), None);
        assert_eq!(Status::from_header(""), None);
    }

    #[test]
    fn aggregate_prefers_worst_status() {
        use std::collections::HashMap;

        let mut statuses = HashMap::new();
        statuses.insert("a".to_string(), Status::Ok);
        statuses.insert("b".to_string(), Status::Ok);
        let agg = crate::app::app_icon::aggregate_status(&statuses);
        assert_eq!(agg, Status::Ok);

        statuses.insert("c".to_string(), Status::Warn);
        let agg = crate::app::app_icon::aggregate_status(&statuses);
        assert_eq!(agg, Status::Warn);

        statuses.insert("d".to_string(), Status::Err);
        let agg = crate::app::app_icon::aggregate_status(&statuses);
        assert_eq!(agg, Status::Err);
    }
}
