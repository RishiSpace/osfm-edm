//! osfm-edm-common — Shared types and data structures for the OSFM-EDM platform.
//!
//! This crate defines the contract between the server, agent, and kernel driver
//! components. All communication types, policy definitions, job payloads, and
//! device models are defined here.

pub mod device;
pub mod events;
pub mod jobs;
pub mod policy;
pub mod protocol;
