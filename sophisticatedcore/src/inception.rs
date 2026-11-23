//! The inception upgrade (InceptionUpgradeWrapper): the backpacks inside a backpack lend it their inventories, and
//! the upgrades in them work as if they sat in the outer backpack. The upgrade picks which inventory comes first.
use mod_api::{ItemStack, Tag};

use crate::contents::{custom_data, get_str, put, set_custom_data};

const KEY: &str = "inventoryOrder";

/// `InventoryOrder`: whose inventory takes items first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Order {
    MainFirst,
    InceptedFirst,
}

impl Order {
    pub fn name(self) -> &'static str {
        match self {
            Order::MainFirst => "main_first",
            Order::InceptedFirst => "incepted_first",
        }
    }

    pub fn next(self) -> Order {
        match self {
            Order::MainFirst => Order::InceptedFirst,
            Order::InceptedFirst => Order::MainFirst,
        }
    }

    /// `fromName`: anything unknown is main first.
    pub fn from_name(name: Option<&str>) -> Order {
        match name {
            Some("incepted_first") => Order::InceptedFirst,
            _ => Order::MainFirst,
        }
    }

    pub fn read(up: &ItemStack) -> Order {
        Order::from_name(get_str(&custom_data(up), KEY).as_deref())
    }

    pub fn write(self, up: &mut ItemStack) {
        let mut d = custom_data(up);
        put(&mut d, KEY, Tag::String(self.name().into()));
        set_custom_data(up, d);
    }
}

/// The order in which a main inventory and its `n` incepted ones are tried: indices, the main one being `n`.
pub fn sequence(order: Order, n: usize) -> Vec<usize> {
    match order {
        Order::MainFirst => std::iter::once(n).chain(0..n).collect(),
        Order::InceptedFirst => (0..=n).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order_cycles_and_reads_names() {
        assert_eq!(Order::MainFirst.next(), Order::InceptedFirst);
        assert_eq!(Order::InceptedFirst.next(), Order::MainFirst);
        assert_eq!(
            Order::from_name(Some("incepted_first")),
            Order::InceptedFirst
        );
        assert_eq!(Order::from_name(Some("junk")), Order::MainFirst);
        assert_eq!(Order::from_name(None), Order::MainFirst);
    }

    #[test]
    fn the_main_inventory_goes_where_the_order_says() {
        assert_eq!(sequence(Order::MainFirst, 2), vec![2, 0, 1]);
        assert_eq!(sequence(Order::InceptedFirst, 2), vec![0, 1, 2]);
        assert_eq!(sequence(Order::MainFirst, 0), vec![0]);
    }
}
