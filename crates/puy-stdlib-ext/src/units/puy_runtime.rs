//! The Rust part of `Puy.Runtime`, which reads the host and isolate memory usage.

#[puy_ext::pascal_module(unit = "Puy.Runtime.Native")]
pub mod native {
    use puy_ext::prelude::*;

    use super::{available, bytes};

    #[pascal_function]
    pub fn process_used(ctx: &mut CallCtx<'_>) -> i64 {
        bytes(ctx.host().memory.as_deref().and_then(|stats| stats.current()))
    }

    #[pascal_function]
    pub fn process_peak(ctx: &mut CallCtx<'_>) -> i64 {
        bytes(ctx.host().memory.as_deref().and_then(|stats| stats.peak()))
    }

    #[pascal_function]
    pub fn process_limit(ctx: &mut CallCtx<'_>) -> i64 {
        bytes(ctx.host().memory.as_deref().and_then(|stats| stats.limit()))
    }

    #[pascal_function]
    pub fn process_available(ctx: &mut CallCtx<'_>) -> i64 {
        ctx.host().memory.as_deref().map_or(-1, available)
    }

    #[pascal_function]
    pub fn isolate_used(ctx: &mut CallCtx<'_>) -> i64 {
        bytes(Some(ctx.isolate_memory().used))
    }

    #[pascal_function]
    pub fn isolate_peak(ctx: &mut CallCtx<'_>) -> i64 {
        bytes(Some(ctx.isolate_memory().peak))
    }

    #[pascal_function]
    pub fn isolate_limit(ctx: &mut CallCtx<'_>) -> i64 {
        bytes(ctx.isolate_memory().limit)
    }

    #[pascal_function]
    pub fn reset_isolate_peak(ctx: &mut CallCtx<'_>) {
        ctx.reset_isolate_peak();
    }

    #[pascal_record(name = "TNativeMemoryTypeUsage")]
    pub struct MemoryTypeUsage {
        pub type_name: String,
        pub count: i64,
        pub bytes: i64,
    }

    #[pascal_function]
    pub fn live_types(ctx: &mut CallCtx<'_>) -> Vec<MemoryTypeUsage> {
        let mut types = ctx.isolate_live_types();
        types.sort_unstable_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.type_name.cmp(&b.type_name)));
        types
            .into_iter()
            .map(|usage| MemoryTypeUsage {
                type_name: usage.type_name,
                count: usage.count as i64,
                bytes: bytes(Some(usage.bytes)),
            })
            .collect()
    }
}

fn bytes(value: Option<usize>) -> i64 {
    value.and_then(|n| i64::try_from(n).ok()).unwrap_or(-1)
}

fn available(stats: &dyn puy_ext::MemoryStats) -> i64 {
    let limit = bytes(stats.limit());
    let used = bytes(stats.current());
    if limit < 0 || used < 0 {
        return -1;
    }
    let reserve = i64::try_from(stats.reserve().unwrap_or(0)).unwrap_or(i64::MAX);
    (limit - reserve).saturating_sub(used).max(0)
}

#[cfg(test)]
mod tests {
    use puy_ext::MemoryStats;

    use super::available;

    struct Stats {
        current: Option<usize>,
        limit: Option<usize>,
        reserve: Option<usize>,
    }

    impl MemoryStats for Stats {
        fn current(&self) -> Option<usize> {
            self.current
        }

        fn peak(&self) -> Option<usize> {
            None
        }

        fn limit(&self) -> Option<usize> {
            self.limit
        }

        fn reserve(&self) -> Option<usize> {
            self.reserve
        }
    }

    // Available process memory uses the host's reserve and clamps the result at zero.
    #[test]
    fn available_with_reserve() {
        for (reserve, expected) in [(None, 60), (Some(10), 50), (Some(110), 0)] {
            let stats = Stats {
                current: Some(40),
                limit: Some(100),
                reserve,
            };
            assert_eq!(available(&stats), expected, "reserve = {reserve:?}");
        }
    }

    // A host may omit either input even if the other one is known.
    #[test]
    fn available_requires_limit_and_current() {
        for (current, limit) in [(None, Some(100)), (Some(40), None)] {
            let stats = Stats {
                current,
                limit,
                reserve: Some(10),
            };
            assert_eq!(available(&stats), -1, "current = {current:?}, limit = {limit:?}");
        }
    }
}
