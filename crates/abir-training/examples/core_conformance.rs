// SPDX-License-Identifier: AGPL-3.0-or-later
//! ADR 0139 P1 core cross-realm conformance oracle.
//!
//! Compiles one registered training profile through the canonical Rust compiler
//! and prints its realm-independent identity as a single line of JSON:
//! `{"canonical_json_hex": "...", "plan_id": "<64 hex>"}`. The compiled
//! execution plan changes execution, never snapshot semantics or the BCS2 wire,
//! so this identity is invariant across every physical realm — exactly the
//! cross-realm claim the core conformance producer attests. Deterministic and
//! allocation-stable: the same profile always yields the same two identities.

use abir_training::{compile_execution_plan, PlanOverrides, TrainingProfile};

fn profile(name: &str) -> TrainingProfile {
    match name {
        "speed" => TrainingProfile::Speed,
        "balanced" => TrainingProfile::Balanced,
        "memory" => TrainingProfile::Memory,
        "compact" => TrainingProfile::Compact,
        "ultra-compact" => TrainingProfile::UltraCompact,
        "stream" => TrainingProfile::Stream,
        other => {
            eprintln!("unknown training profile: {other}");
            std::process::exit(2);
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write;
        write!(&mut out, "{byte:02x}").expect("string write cannot fail");
    }
    out
}

fn main() {
    let name = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: core_conformance <profile>");
        std::process::exit(2);
    });
    let plan = compile_execution_plan(profile(&name), PlanOverrides::default())
        .expect("registered profile compiles");
    let canonical = plan.canonical_json().expect("canonical json");
    let plan_id = plan.content_id().expect("plan content id").to_string();
    // Single deterministic JSON line on stdout; nothing else.
    println!(
        "{{\"canonical_json_hex\":\"{}\",\"plan_id\":\"{}\"}}",
        hex(&canonical),
        plan_id
    );
}
