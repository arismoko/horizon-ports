//! The bucket of experience (`xp_bucket`): an item holding 1000 mB of `xp_still`, as NeoForge's `FluidBucketWrapper`
//! holds a filled bucket. It is one tank that only gives its fluid up whole, and then becomes an empty bucket.
//! Nothing can be put into it, and using it on a block does nothing.

use mod_api::{
    FluidKey, FluidResource, FluidStack, ItemKey, ItemPlace, ItemResource, ItemStack,
    ResourceHandler, ServerHost, Transaction,
};

/// What a bucket holds.
const BUCKET: u32 = 1000;

pub struct XpBucket {
    pub place: ItemPlace,
    pub xp: FluidKey,
    pub empty: ItemKey,
}

/// Whether a drain of `amount` of a fluid takes the bucket's contents: the same fluid, and the whole bucket.
fn drains(same_fluid: bool, amount: u32) -> bool {
    same_fluid && amount >= BUCKET
}

impl ResourceHandler<FluidResource> for XpBucket {
    fn size(&self, _: &mut dyn ServerHost) -> usize {
        1
    }

    fn resource(&self, _: &mut dyn ServerHost, index: usize) -> Option<FluidResource> {
        (index == 0).then(|| FluidResource::of(&FluidStack::new(self.xp, 1)))
    }

    fn amount(&self, _: &mut dyn ServerHost, index: usize) -> u32 {
        if index == 0 { BUCKET } else { 0 }
    }

    fn capacity(&self, _: &mut dyn ServerHost, index: usize, _: Option<&FluidResource>) -> u32 {
        if index == 0 { BUCKET } else { 0 }
    }

    fn insert_at(
        &self,
        _: &mut dyn ServerHost,
        _: &Transaction,
        _: usize,
        _: &FluidResource,
        _: u32,
    ) -> u32 {
        0
    }

    fn extract_at(
        &self,
        host: &mut dyn ServerHost,
        tx: &Transaction,
        index: usize,
        resource: &FluidResource,
        amount: u32,
    ) -> u32 {
        let same = index == 0 && resource.fluid().key() == Some(self.xp);
        if !drains(same, amount) {
            return 0;
        }
        let empty = ItemResource::of(&ItemStack::of(self.empty));
        match self.place.exchange(host, tx, &empty, 1) {
            1 => BUCKET,
            _ => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::drains;

    #[test]
    fn only_a_whole_bucket_of_experience_drains() {
        assert!(drains(true, 1000));
        assert!(drains(true, 2000));
        assert!(!drains(true, 999));
        assert!(!drains(false, 1000));
    }
}
