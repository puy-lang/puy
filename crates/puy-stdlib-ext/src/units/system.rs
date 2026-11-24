//! Native functions called by the Pascal part of `System`.

#[puy_ext::pascal_module(unit = "System.Native")]
pub mod native {
    use puy_ext::prelude::*;

    #[pascal_function]
    pub fn memory_manager_live_block_count(ctx: &mut CallCtx<'_>) -> u32 {
        u32::try_from(ctx.isolate_live_block_count()).unwrap_or(u32::MAX)
    }

    #[pascal_function]
    pub fn memory_manager_isolate_used(ctx: &mut CallCtx<'_>) -> u64 {
        ctx.isolate_memory().used as u64
    }

    #[pascal_function]
    pub fn memory_manager_live_block_bytes(ctx: &mut CallCtx<'_>) -> u64 {
        ctx.isolate_live_block_bytes() as u64
    }
}
