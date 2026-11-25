//! The typed state of the block entities that keep one (the decoration table, the storage controller and the
//! network nodes): one `Compound` attachment each, issued to `register` and read and edited by the code that owns
//! it. Part state stays on the server, so none of these syncs.
use std::sync::RwLock;

use mod_api::{
    AttachmentCodec, AttachmentDescriptor, AttachmentKey, BlockEntityRef, Compound, Host,
    PartStateError, Registrar, ServerHost, edit_part_state, read_state,
};

/// The key a registration pass issued for one state.
pub struct StateKey {
    id: &'static str,
    key: RwLock<Option<AttachmentKey<Compound>>>,
}

impl StateKey {
    pub const fn new(id: &'static str) -> Self {
        Self {
            id,
            key: RwLock::new(None),
        }
    }

    /// Declares the state's attachment type and keeps its key; the key a part is built with.
    pub fn register(&self, r: &mut dyn Registrar) -> Option<AttachmentKey<Compound>> {
        let key = r
            .register_attachment_type(AttachmentDescriptor::new(self.id, Compound::new).serialize(
                AttachmentCodec {
                    encode: |c| Ok(c.clone()),
                    decode: |c| Ok(c.clone()),
                },
            ))
            .map_err(|e| eprintln!("[sophisticatedcore] attachment {}: {e}", self.id))
            .ok()?;
        *self.key.write().unwrap_or_else(|e| e.into_inner()) = Some(key.clone());
        Some(key)
    }

    fn key(&self) -> Option<AttachmentKey<Compound>> {
        self.key.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Reads the block entity's state (the default one when it has none yet).
    pub fn read<R>(
        &self,
        host: &dyn Host,
        be: BlockEntityRef,
        read: impl FnOnce(&Compound) -> R,
    ) -> Option<R> {
        read_state(host, be, &self.key()?, |c| match c {
            Some(c) => read(c),
            None => read(&Compound::new()),
        })
        .ok()
    }

    /// Edits the block entity's state in a transaction of its own.
    pub fn edit(
        &self,
        host: &mut dyn ServerHost,
        be: BlockEntityRef,
        edit: impl FnOnce(&mut Compound),
    ) -> Result<(), PartStateError> {
        let key = self.key().ok_or(PartStateError::Unavailable)?;
        edit_part_state(host, be, &key, |_, _, c| edit(c))
    }
}
