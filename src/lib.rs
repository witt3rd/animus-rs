//! # animus-rs
//!
//! Substrate for relational beings — the machinery that lets an animus exist,
//! persist, and become.
//!
//! Data plane (work queues via pgmq, semantic memory via pgvector), control
//! plane (queue watching, resource gating, focus spawning), skills
//! (pluggable cognitive specializations), LLM abstraction, and
//! observability (OpenTelemetry). All on Postgres.

pub mod config;
pub mod db;
pub mod engine;
pub mod error;
pub mod faculty;
pub mod llm;
pub mod memory;
pub mod model;
pub mod telemetry;
