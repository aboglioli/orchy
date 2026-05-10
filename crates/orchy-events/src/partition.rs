use std::num::NonZeroU32;

pub trait PartitionKey {
    fn partition(&self, total_partitions: NonZeroU32) -> u32;
}
