//! Software inventory collection — collects installed packages on Linux.

use osfm_edm_common::protocol::SoftwareItem;

/// Collect installed software packages.
pub fn collect_software() -> Vec<SoftwareItem> {
    if cfg!(target_os = "linux") {
        collect_dpkg()
            .or_else(|| collect_rpm())
            .unwrap_or_default()
    } else {
        Vec::new()
    }
}

/// Collect from dpkg (Debian/Ubuntu).
fn collect_dpkg() -> Option<Vec<SoftwareItem>> {
    let output = std::process::Command::new("dpkg-query")
        .args(["-W", "-f", "${Package}\t${Version}\t${Status}\n"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let items: Vec<SoftwareItem> = text
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 2 && parts.get(2).map_or(true, |s| s.contains("installed")) {
                Some(SoftwareItem {
                    name: parts[0].to_string(),
                    version: Some(parts[1].to_string()),
                    publisher: None,
                    install_date: None,
                })
            } else {
                None
            }
        })
        .collect();

    Some(items)
}

/// Collect from rpm (RHEL/Fedora).
fn collect_rpm() -> Option<Vec<SoftwareItem>> {
    let output = std::process::Command::new("rpm")
        .args(["-qa", "--queryformat", "%{NAME}\t%{VERSION}-%{RELEASE}\t%{VENDOR}\n"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let items: Vec<SoftwareItem> = text
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 2 {
                Some(SoftwareItem {
                    name: parts[0].to_string(),
                    version: Some(parts[1].to_string()),
                    publisher: parts.get(2).map(|s| s.to_string()),
                    install_date: None,
                })
            } else {
                None
            }
        })
        .collect();

    Some(items)
}
