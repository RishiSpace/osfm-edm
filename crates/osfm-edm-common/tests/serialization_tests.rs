//! Tests for osfm-edm-common serialization round-trips.

#[cfg(test)]
mod tests {
    use osfm_edm_common::events::{FileOperation, KernelEvent, NetworkProtocol};
    use osfm_edm_common::jobs::{JobPayload, JobStatus, ShellType};
    use osfm_edm_common::policy::{PolicyDefinition, PolicyRule, UpdatePolicy};
    use osfm_edm_common::protocol::{AgentMessage, ServerMessage, TelemetrySnapshot};
    use uuid::Uuid;

    #[test]
    fn kernel_event_round_trip() {
        let event = KernelEvent::ProcessStarted {
            pid: 1234,
            ppid: 1,
            path: "/usr/bin/ls".to_string(),
            cmdline: "ls -la".to_string(),
            user: Some("root".to_string()),
            timestamp: 1700000000,
        };
        let json = serde_json::to_string(&event).unwrap();
        let deserialized: KernelEvent = serde_json::from_str(&json).unwrap();
        match deserialized {
            KernelEvent::ProcessStarted { pid, ppid, path, .. } => {
                assert_eq!(pid, 1234);
                assert_eq!(ppid, 1);
                assert_eq!(path, "/usr/bin/ls");
            }
            _ => panic!("Unexpected variant"),
        }
    }

    #[test]
    fn kernel_event_tagged_serialization() {
        let event = KernelEvent::FileAccessed {
            pid: 42,
            path: "/etc/passwd".to_string(),
            operation: FileOperation::Read,
            timestamp: 1700000000,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""type":"file_accessed"#));
        assert!(json.contains(r#""operation":"read"#));
    }

    #[test]
    fn network_event_round_trip() {
        let event = KernelEvent::NetworkConnected {
            pid: 99,
            src: "192.168.1.10:50000".to_string(),
            dst: "93.184.216.34:443".to_string(),
            protocol: NetworkProtocol::Tcp,
            timestamp: 1700000000,
        };
        let json = serde_json::to_string(&event).unwrap();
        let de: KernelEvent = serde_json::from_str(&json).unwrap();
        match de {
            KernelEvent::NetworkConnected { protocol, .. } => {
                let proto_json = serde_json::to_string(&protocol).unwrap();
                assert_eq!(proto_json, r#""tcp""#);
            }
            _ => panic!("Unexpected variant"),
        }
    }

    #[test]
    fn policy_round_trip() {
        let policy = PolicyDefinition {
            id: Uuid::new_v4(),
            name: "Test Policy".to_string(),
            rules: vec![
                PolicyRule::ScreenLock {
                    timeout_minutes: 5,
                    require_password: true,
                },
                PolicyRule::Firewall { enabled: true },
                PolicyRule::OsUpdate {
                    auto_install: UpdatePolicy::SecurityOnly,
                    reboot_window: Some("02:00-04:00".to_string()),
                },
                PolicyRule::ProcessBlacklist {
                    deny: vec!["bitcoin-miner".to_string()],
                },
                PolicyRule::UsbStorage { allow: false },
            ],
        };
        let json = serde_json::to_string(&policy).unwrap();
        let de: PolicyDefinition = serde_json::from_str(&json).unwrap();
        assert_eq!(de.name, "Test Policy");
        assert_eq!(de.rules.len(), 5);
    }

    #[test]
    fn job_payload_round_trip() {
        let payloads = vec![
            JobPayload::RunScript {
                shell: ShellType::Bash,
                script: "echo hello".to_string(),
            },
            JobPayload::CollectInventory,
            JobPayload::Reboot { delay_seconds: 30 },
        ];
        for payload in &payloads {
            let json = serde_json::to_string(payload).unwrap();
            let de: JobPayload = serde_json::from_str(&json).unwrap();
            let json2 = serde_json::to_string(&de).unwrap();
            assert_eq!(json, json2);
        }
    }

    #[test]
    fn job_status_display() {
        assert_eq!(JobStatus::Pending.to_string(), "pending");
        assert_eq!(JobStatus::Running.to_string(), "running");
        assert_eq!(JobStatus::Done.to_string(), "done");
        assert_eq!(JobStatus::Failed.to_string(), "failed");
        assert_eq!(JobStatus::Cancelled.to_string(), "cancelled");
    }

    #[test]
    fn server_message_round_trip() {
        let msg = ServerMessage::DispatchJob {
            job_id: Uuid::new_v4(),
            payload: JobPayload::RunScript {
                shell: ShellType::Powershell,
                script: "Get-Process".to_string(),
            },
            signature: "deadbeef".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""msg_type":"dispatch_job"#));
        let de: ServerMessage = serde_json::from_str(&json).unwrap();
        match de {
            ServerMessage::DispatchJob { signature, .. } => {
                assert_eq!(signature, "deadbeef");
            }
            _ => panic!("Unexpected variant"),
        }
    }

    #[test]
    fn agent_message_telemetry() {
        let msg = AgentMessage::TelemetryReport {
            snapshot: TelemetrySnapshot {
                cpu_pct: 42.5,
                ram_used_mb: 8192,
                ram_total_mb: 16384,
                disk_used_gb: 250.0,
                disk_total_gb: 500.0,
                uptime_secs: 86400,
                timestamp: 1700000000,
            },
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""msg_type":"telemetry_report"#));
        let de: AgentMessage = serde_json::from_str(&json).unwrap();
        match de {
            AgentMessage::TelemetryReport { snapshot } => {
                assert!((snapshot.cpu_pct - 42.5).abs() < f64::EPSILON);
                assert_eq!(snapshot.ram_used_mb, 8192);
            }
            _ => panic!("Unexpected variant"),
        }
    }

    #[test]
    fn agent_message_heartbeat() {
        let msg = AgentMessage::Heartbeat {
            agent_version: "0.1.0".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        let de: AgentMessage = serde_json::from_str(&json).unwrap();
        match de {
            AgentMessage::Heartbeat { agent_version } => {
                assert_eq!(agent_version, "0.1.0");
            }
            _ => panic!("Unexpected variant"),
        }
    }
}
