use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub version: String,
    #[serde(default = "default_poll_interval")]
    pub poll_interval_secs: u64,
    #[serde(default = "default_autostart")]
    pub autostart: bool,
    #[serde(default = "default_nice")]
    pub default_nice: i8,
    #[serde(default = "default_ionice_class")]
    pub default_ionice_class: String,
    #[serde(default)]
    pub icons: IconConfig,
    pub targets: Vec<TargetConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct IconConfig {
    pub ok: String,
    pub warn: String,
    pub err: String,
}

impl Default for IconConfig {
    fn default() -> Self {
        Self {
            ok: "icon-ok.svg".to_string(),
            warn: "icon-warn.svg".to_string(),
            err: "icon-err.svg".to_string(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TargetConfig {
    pub name: String,
    pub label: Option<String>,
    pub script: Option<String>,
    pub log: Option<String>,
    #[serde(rename = "pid_file")]
    pub pid_file: Option<String>,
    #[serde(rename = "pid_pattern")]
    pub pid_pattern: Option<String>,
    #[serde(rename = "restart_script")]
    pub restart_script: Option<String>,
    #[serde(rename = "stoppage_after_secs")]
    pub stoppage_after_secs: Option<u64>,
}

fn default_poll_interval() -> u64 {
    5
}

fn default_autostart() -> bool {
    true
}

fn default_nice() -> i8 {
    10
}

fn default_ionice_class() -> String {
    "idle".to_string()
}

impl Config {
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.version == "1",
            "unsupported config version: {}",
            self.version
        );

        let mut names = std::collections::HashSet::new();
        for target in &self.targets {
            anyhow::ensure!(!target.name.is_empty(), "target name must not be empty");
            anyhow::ensure!(
                names.insert(target.name.clone()),
                "duplicate target name: {}",
                target.name
            );
            anyhow::ensure!(
                target.script.is_some() || target.log.is_some(),
                "target '{}' must have at least one of 'script' or 'log'",
                target.name
            );
            anyhow::ensure!(
                !(target.pid_file.is_some() && target.pid_pattern.is_some()),
                "target '{}' has both 'pid_file' and 'pid_pattern'; use one",
                target.name
            );
            if let Some(ref script) = target.restart_script {
                if !std::path::Path::new(script).exists() {
                    eprintln!(
                        "warning: restart_script '{}' for target '{}' does not exist",
                        script, target.name
                    );
                }
            }
        }

        let valid_classes = ["idle", "best-effort", "realtime"];
        anyhow::ensure!(
            valid_classes.contains(&self.default_ionice_class.as_str()),
            "default_ionice_class must be one of: {}",
            valid_classes.join(", ")
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_yaml() -> String {
        r#"
version: "1"
targets:
  - name: backup
    script: /usr/local/bin/backup-check.sh
    log: /var/log/backup.log
    pid_file: /run/backup.pid
"#
        .to_string()
    }

    #[test]
    fn parses_minimal_config_with_defaults() {
        let yaml = r#"
version: "1"
targets:
  - name: job
    script: /bin/true
"#;
        let config: Config = serde_yaml::from_str(yaml).expect("parse failed");
        assert_eq!(config.poll_interval_secs, 5);
        assert!(config.autostart);
        assert_eq!(config.default_nice, 10);
        assert_eq!(config.default_ionice_class, "idle");
        assert_eq!(config.icons.ok, "icon-ok.svg");
        assert!(config.validate().is_ok());
    }

    #[test]
    fn validates_valid_config() {
        let config: Config = serde_yaml::from_str(&valid_yaml()).unwrap();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn rejects_wrong_version() {
        let yaml = valid_yaml().replacen("\"1\"", "\"2\"", 1);
        let config: Config = serde_yaml::from_str(&yaml).unwrap();
        assert!(config.validate().is_err());
    }

    #[test]
    fn rejects_target_without_script_and_log() {
        let yaml = r#"
version: "1"
targets:
  - name: job
"#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert!(config.validate().is_err());
    }

    #[test]
    fn rejects_duplicate_names() {
        let yaml = r#"
version: "1"
targets:
  - name: job
    script: /bin/true
  - name: job
    log: /var/log/job.log
"#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert!(config.validate().is_err());
    }

    #[test]
    fn rejects_both_pid_file_and_pattern() {
        let yaml = r#"
version: "1"
targets:
  - name: job
    script: /bin/true
    pid_file: /run/job.pid
    pid_pattern: "job.*"
"#;
        let config: Config = serde_yaml::from_str(yaml).unwrap();
        assert!(config.validate().is_err());
    }

    #[test]
    fn rejects_bad_ionice_class() {
        let yaml = valid_yaml().replace("targets:", "default_ionice_class: bogus\ntargets:");
        let config: Config = serde_yaml::from_str(&yaml).unwrap();
        assert!(config.validate().is_err());
    }
}
