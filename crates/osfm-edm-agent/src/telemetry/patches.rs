//! Patch status collection — identifies pending updates on Linux.

use osfm_edm_common::protocol::PatchItem;

/// Collect pending patches/updates.
pub fn collect_patches() -> Vec<PatchItem> {
    if cfg!(target_os = "linux") {
        collect_apt_upgradable()
            .or_else(|| collect_dnf_updates())
            .unwrap_or_default()
    } else {
        Vec::new()
    }
}

/// Collect from apt (Debian/Ubuntu).
fn collect_apt_upgradable() -> Option<Vec<PatchItem>> {
    // Run apt list --upgradable.
    let output = std::process::Command::new("apt")
        .args(["list", "--upgradable"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let items: Vec<PatchItem> = text
        .lines()
        .skip(1) // Skip "Listing..." header
        .filter_map(|line| {
            // Format: package/source version arch [upgradable from: old_version]
            let slash = line.find('/')?;
            let name = &line[..slash];
            let rest = &line[slash + 1..];
            let version = rest.split_whitespace().nth(1).unwrap_or("unknown");
            Some(PatchItem {
                patch_id: name.to_string(),
                title: Some(format!("{name} → {version}")),
                severity: None,
                status: "available".to_string(),
            })
        })
        .collect();

    Some(items)
}

/// Collect from dnf (RHEL/Fedora).
fn collect_dnf_updates() -> Option<Vec<PatchItem>> {
    let output = std::process::Command::new("dnf")
        .args(["check-update", "--quiet"])
        .output()
        .ok()?;

    // dnf check-update returns exit code 100 if updates are available.
    let text = String::from_utf8_lossy(&output.stdout);
    let items: Vec<PatchItem> = text
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                Some(PatchItem {
                    patch_id: parts[0].to_string(),
                    title: Some(format!("{} → {}", parts[0], parts[1])),
                    severity: None,
                    status: "available".to_string(),
                })
            } else {
                None
            }
        })
        .collect();

    Some(items)
}
