//! Software inventory collection — collects installed packages on Linux.

use osfm_edm_common::protocol::SoftwareItem;

/// Collect installed software packages.
pub fn collect_software() -> Vec<SoftwareItem> {
    if cfg!(target_os = "linux") {
        collect_dpkg().or_else(collect_rpm).unwrap_or_default()
    } else if cfg!(target_os = "windows") {
        collect_winget().unwrap_or_default()
    } else if cfg!(target_os = "macos") {
        collect_brew().unwrap_or_default()
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
            if parts.len() >= 2 && parts.get(2).is_none_or(|s| s.contains("installed")) {
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
        .args([
            "-qa",
            "--queryformat",
            "%{NAME}\t%{VERSION}-%{RELEASE}\t%{VENDOR}\n",
        ])
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

/// Collect from winget (Windows) — `winget list` tabular output.
fn collect_winget() -> Option<Vec<SoftwareItem>> {
    let output = std::process::Command::new("winget")
        .args(["list", "--disable-interactivity"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut items = Vec::new();
    for line in text.lines().skip(2) {
        let line = line.trim_end();
        if line.len() < 8 || line.chars().all(|c| c == '-') {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 {
            items.push(SoftwareItem {
                name: parts[..parts.len() - 2].join(" "),
                version: Some(parts[parts.len() - 2].to_string()),
                publisher: None,
                install_date: None,
            });
        }
    }
    Some(items)
}

/// Collect from Homebrew (macOS) — `brew list --versions`.
fn collect_brew() -> Option<Vec<SoftwareItem>> {
    let output = std::process::Command::new("brew")
        .args(["list", "--versions"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let items = text
        .lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let name = parts.next()?;
            let version = parts.next().map(|s| s.to_string());
            Some(SoftwareItem {
                name: name.to_string(),
                version,
                publisher: None,
                install_date: None,
            })
        })
        .collect();
    Some(items)
}
