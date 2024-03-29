use super::id::{ProcessId, ShardId};

/// Returns an iterator with all process identifiers in this shard in a system
/// with `n` processes.
pub fn process_ids(shard_id: ShardId, n: usize) -> impl Iterator<Item = ProcessId> {
    // compute process identifiers, making sure ids are non-zero
    let shift = n * usize::try_from(shard_id).expect("truncate error");
    (1..=n).map(move |id| ProcessId::try_from(id + shift).expect("truncate error"))
}

pub fn all_process_ids(shard_count: usize, n: usize) -> impl Iterator<Item = (ProcessId, ShardId)> {
    (0..shard_count).flat_map(move |shard_id| {
        let shard_id = shard_id as ShardId;
        process_ids(shard_id, n).map(move |process_id| (process_id, shard_id))
    })
}
