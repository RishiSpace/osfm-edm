//! osfm-edm-common — Shared types and data structures for the OSFM-EDM platform.
//!
//! This crate defines the contract between the server and agent components.
//! All communication types, policy definitions, job payloads, system event
//! definitions, and device models are defined here.

pub mod device;
pub mod events;
pub mod jobs;
pub mod policy;
pub mod protocol;
