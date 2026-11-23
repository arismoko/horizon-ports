//! A backpack's tank and battery offered to pipes (BackpackBlockEntity's and BackpackWrapperLookup's fluid and
//! energy handlers): a placed backpack answers the block `FLUIDS` and `ENERGY` capabilities, and the backpack as an
//! item answers `ITEM_FLUIDS` (when `item_fluid_handler_enabled` is on) and `ITEM_ENERGY`. The tank is the first tank
//! upgrade and the battery the first battery upgrade; with none, the handler holds nothing. Transfers are limited by
//! the upgrade's own rate, as the original's wrappers limit them.

use mod_api::{
    EnergyHandler, FluidResource, ItemPlace, ItemResource, ItemStack, LevelRef, ResourceHandler,
    ServerHost, Transaction,
};

use crate::contents::Contents;
use crate::fluids::{self, Limits, Tank};
use crate::storage_items::StorageItem;
use crate::upgrades::{self, Kind};

/// Where the backpack's stack is.
enum Src {
    Block(LevelRef, [i32; 3]),
    Item(ItemPlace),
}

pub struct Backed {
    kind: &'static StorageItem,
    src: Src,
}

type Loaded = (ItemStack, Contents);

impl Backed {
    pub fn block(kind: &'static StorageItem, level: LevelRef, pos: [i32; 3]) -> Backed {
        Backed {
            kind,
            src: Src::Block(level, pos),
        }
    }

    pub fn item(kind: &'static StorageItem, place: ItemPlace) -> Backed {
        Backed {
            kind,
            src: Src::Item(place),
        }
    }

    fn load(&self, host: &mut dyn ServerHost) -> Option<Loaded> {
        let st = match &self.src {
            Src::Block(level, pos) => crate::storage::read_be(&*host, *level, *pos)?,
            Src::Item(place) => place.with_stack(host, |v| v.project()).flatten()?,
        };
        let c = Contents::read(&*host, &st, self.kind);
        Some((st, c))
    }

    fn save(
        &self,
        host: &mut dyn ServerHost,
        tx: &Transaction,
        mut st: ItemStack,
        c: &Contents,
    ) -> bool {
        c.write(&*host, &mut st, self.kind);
        match &self.src {
            Src::Block(level, pos) => host
                .block_entity(*level, *pos)
                .and_then(|be| be.set_item_slot(host, tx, 0, Some(st)).ok())
                .unwrap_or(false),
            Src::Item(place) => place.exchange(host, tx, &ItemResource::of(&st), 1) == 1,
        }
    }

    /// Runs `f` on the first upgrade of `kind` (its stack and the storage's limits) and saves what it changed.
    fn with_upgrade<R: Default>(
        &self,
        host: &mut dyn ServerHost,
        tx: Option<&Transaction>,
        kind: Kind,
        f: impl FnOnce(&mut ItemStack, &Limits) -> (R, bool),
    ) -> R {
        let Some((st, mut c)) = self.load(host) else {
            return R::default();
        };
        let Some(j) = c.upgrades.iter().position(|u| {
            u.as_ref()
                .and_then(|u| upgrades::of(&*host, u))
                .is_some_and(|u| u.kind == kind)
        }) else {
            return R::default();
        };
        let l = fluids::limits(&*host, self.kind, &c);
        let mut up = c.upgrades[j].clone().expect("found above");
        let (out, changed) = f(&mut up, &l);
        if changed && let Some(tx) = tx {
            c.upgrades[j] = Some(up);
            if !self.save(host, tx, st, &c) {
                return R::default();
            }
        }
        out
    }

    fn tank(&self, host: &mut dyn ServerHost) -> (Tank, u32) {
        let (t, cap) = self.with_upgrade(host, None, Kind::Tank, |up, l| {
            ((fluids::read_tank(up), l.tank_cap), false)
        });
        (t, cap)
    }
}

impl ResourceHandler<FluidResource> for Backed {
    fn size(&self, host: &mut dyn ServerHost) -> usize {
        usize::from(self.tank(host).1 > 0)
    }
    fn resource(&self, host: &mut dyn ServerHost, _index: usize) -> Option<FluidResource> {
        let id = self.tank(host).0.fluid?;
        let key = host.fluid_key(&id)?;
        Some(FluidResource::of(&mod_api::FluidStack::new(key, 1)))
    }
    fn amount(&self, host: &mut dyn ServerHost, _index: usize) -> u32 {
        self.tank(host).0.amount
    }
    fn capacity(
        &self,
        host: &mut dyn ServerHost,
        _index: usize,
        _resource: Option<&FluidResource>,
    ) -> u32 {
        self.tank(host).1
    }
    fn insert_at(
        &self,
        host: &mut dyn ServerHost,
        tx: &Transaction,
        index: usize,
        resource: &FluidResource,
        amount: u32,
    ) -> u32 {
        if index != 0 {
            return 0;
        }
        let Some(id) = resource
            .to_stack(1)
            .key()
            .and_then(|k| k.id(&*host).map(str::to_owned))
        else {
            return 0;
        };
        self.with_upgrade(host, Some(tx), Kind::Tank, |up, l| {
            let mut t = fluids::read_tank(up);
            let n = t.fill(&id, amount, l.tank_cap, Some(l.tank_io));
            if n > 0 {
                fluids::write_tank(up, &t);
            }
            (n, n > 0)
        })
    }
    fn extract_at(
        &self,
        host: &mut dyn ServerHost,
        tx: &Transaction,
        index: usize,
        resource: &FluidResource,
        amount: u32,
    ) -> u32 {
        if index != 0 {
            return 0;
        }
        let id = resource
            .to_stack(1)
            .key()
            .and_then(|k| k.id(&*host).map(str::to_owned));
        self.with_upgrade(host, Some(tx), Kind::Tank, |up, l| {
            let mut t = fluids::read_tank(up);
            if t.fluid != id {
                return (0, false);
            }
            let n = t.drain(amount, Some(l.tank_io));
            if n > 0 {
                fluids::write_tank(up, &t);
            }
            (n, n > 0)
        })
    }
}

impl EnergyHandler for Backed {
    fn amount(&self, host: &mut dyn ServerHost) -> u32 {
        self.with_upgrade(host, None, Kind::Battery, |up, _| {
            (fluids::read_energy(up), false)
        })
    }
    fn capacity(&self, host: &mut dyn ServerHost) -> u32 {
        self.with_upgrade(host, None, Kind::Battery, |_, l| (l.battery_cap, false))
    }
    fn insert(&self, host: &mut dyn ServerHost, tx: &Transaction, amount: u32) -> u32 {
        self.with_upgrade(host, Some(tx), Kind::Battery, |up, l| {
            let e = fluids::read_energy(up);
            let n = amount
                .min(l.battery_cap.saturating_sub(e))
                .min(l.battery_io);
            if n > 0 {
                fluids::write_energy(up, e + n);
            }
            (n, n > 0)
        })
    }
    fn extract(&self, host: &mut dyn ServerHost, tx: &Transaction, amount: u32) -> u32 {
        self.with_upgrade(host, Some(tx), Kind::Battery, |up, l| {
            let e = fluids::read_energy(up);
            let n = amount.min(e).min(l.battery_io);
            if n > 0 {
                fluids::write_energy(up, e - n);
            }
            (n, n > 0)
        })
    }
}
