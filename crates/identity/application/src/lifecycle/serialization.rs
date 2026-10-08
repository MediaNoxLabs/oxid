// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, Weak},
};

use oxid_identity_domain::{IdentityProfileId, MidnightDid};

use crate::{DidLifecyclePortError, DidOperationError};

type Scope = (IdentityProfileId, MidnightDid);
type Locks = BTreeMap<Scope, Weak<Mutex<()>>>;

// Shared across service instances: recomposing over the same repository must not
// create a second serialization domain. Only the registry lookup is global;
// effects on unrelated profile/DID pairs remain concurrent. Dead entries are
// reclaimed, while every holder and waiter keeps its exact lock alive.
static LOCKS: Mutex<Locks> = Mutex::new(BTreeMap::new());

pub(super) fn operation_lock(
    profile: &IdentityProfileId,
    did: &MidnightDid,
) -> Result<Arc<Mutex<()>>, DidOperationError> {
    let mut locks = LOCKS.lock().map_err(|_| unavailable())?;
    locks.retain(|_, lock| lock.strong_count() > 0);
    let entry = locks.entry((profile.clone(), did.clone())).or_default();
    if let Some(lock) = entry.upgrade() {
        return Ok(lock);
    }
    let lock = Arc::new(Mutex::new(()));
    *entry = Arc::downgrade(&lock);
    Ok(lock)
}

pub(super) fn unavailable() -> DidOperationError {
    DidOperationError::Lifecycle(DidLifecyclePortError::Unavailable)
}
