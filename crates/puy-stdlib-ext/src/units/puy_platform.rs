//! The Rust part of `Puy.Platform`: the unit `Puy.Platform.Native`, which the Pascal unit uses in
//! its `implementation` part (specification: `docs/spec/stdlib/Puy.Platform/README.md`).
//!
//! `TCapabilities` answers with what the isolate's `HostSet` has: each capability present, or
//! why it is missing (`docs/design/stdlib.md` 3.4.2).

#[puy_ext::pascal_module(unit = "Puy.Platform.Native")]
pub mod native {
    use puy_ext::prelude::*;
    use puy_ext::{Absent, Capability};

    /// The state of the capability `index` (an ordinal of `TCapability`, in the order of
    /// `Capability::ALL`): 0 present, 1 not on this target, 2 not granted by the embedder (the
    /// ordinals of `TCapabilityStatus`).
    #[pascal_function]
    pub fn capability_status(ctx: &mut CallCtx<'_>, index: i32) -> Result<i32, PasError> {
        let cap = usize::try_from(index)
            .ok()
            .and_then(|i| Capability::ALL.get(i).copied())
            .ok_or_else(|| PasError::range_error(format!("{index} is not a capability")))?;
        Ok(match ctx.host().absent(cap) {
            None => 0,
            Some(Absent::Unsupported) => 1,
            Some(Absent::NoPermission) => 2,
        })
    }
}
