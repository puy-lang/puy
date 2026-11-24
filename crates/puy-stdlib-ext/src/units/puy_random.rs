//! The Rust part of `Puy.Random`: the unit `Puy.Random.Native`, which the Pascal unit uses in
//! its `implementation` part (specification: `docs/spec/stdlib/Puy.Random/README.md`).
//!
//! The generator itself is Pascal; Rust only reads the seed of `TRandom.Create` from the
//! isolate's source of random numbers (requirements §8.9).

#[puy_ext::pascal_module(unit = "Puy.Random.Native")]
pub mod native {
    use puy_ext::prelude::*;

    /// A seed from 8 bytes of the host's source of random numbers, little-endian. Without a
    /// source, `ENotSupportedException` with the reason of its absence (`ekUnsupported` or
    /// `ekNoPermission`); a failure of the source is the exception of its host error
    /// (`docs/spec/bytecode.md` chapter 4: mostly `EInOutError`).
    #[pascal_function]
    pub fn os_seed(ctx: &mut CallCtx<'_>) -> Result<i64, PasError> {
        let host = ctx.host();
        let absent = host.absent.random;
        let random = host
            .random
            .as_deref_mut()
            .ok_or_else(|| absent.error("the source of random numbers"))?;
        let mut buf = [0; 8];
        random.fill(&mut buf)?;
        Ok(i64::from_le_bytes(buf))
    }
}
