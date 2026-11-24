// SPDX-License-Identifier: GPL-3.0-or-later

//! The import generator skips emitting an adapter for a symbol it classifies
//! as a standard service, so every such classification must name a handler
//! the standard loader registry actually provides.

use std::sync::Arc;
use xenora_import_gen::{Classification, SymbolDb};
use xenora_loader::import_table::ImportName;
use xenora_loader::imports::{ImportRegistry, ServiceStatus};
use xenora_loader::services::{MockClock, ServicesState, register_standard_services};

#[test]
fn every_standard_service_classification_resolves_to_a_registered_handler() {
    let mut registry = ImportRegistry::new();
    let state = ServicesState::new(Arc::new(MockClock::new(0)));
    register_standard_services(&mut registry, Arc::new(state));

    let db = SymbolDb::builtin();
    let mut checked = 0;
    let mut missing = Vec::new();
    for (module, symbol, class) in db.entries() {
        if !matches!(class, Classification::StandardService) {
            continue;
        }
        checked += 1;
        match registry.resolve(module, &ImportName::Name(symbol.to_string())) {
            Some(service) if service.status != ServiceStatus::Stub => {}
            _ => missing.push(format!("{module}!{symbol}")),
        }
    }
    assert!(missing.is_empty(), "unregistered: {missing:?}");
    assert!(checked > 100, "{checked}");
}

#[test]
fn registered_synchronization_services_are_not_classified_as_stubs() {
    let db = SymbolDb::builtin();
    for module in ["api-ms-win-core-synch-l1-1-0.dll", "kernel32.dll"] {
        for symbol in [
            "CreateSemaphoreW",
            "ReleaseSemaphore",
            "CreateMutexW",
            "EnterCriticalSection",
            "TryEnterCriticalSection",
            "AcquireSRWLockExclusive",
        ] {
            assert!(
                matches!(
                    db.get(module, symbol),
                    Some(Classification::StandardService)
                ),
                "{module}!{symbol}"
            );
        }
    }
}
