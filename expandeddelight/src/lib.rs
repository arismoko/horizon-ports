//! Expanded Delight's recipe type. The juicer itself (block entity `farmersdelight:juicer`) runs in
//! native-next/farmersdelight, which reads `expandeddelight:juicing` recipes as raw JSON.
use mod_api::{Hooks, Registrar, RegistrationContext};

#[derive(Default)]
pub struct Mod;

const JUICING: &str = "expandeddelight:juicing";

#[mod_api::horizon_mod]
impl Hooks for Mod {
    fn register(&self, r: &mut dyn Registrar, _cx: &RegistrationContext) {
        r.register_recipe_type(JUICING);
        r.register_raw_recipe_serializer(JUICING, JUICING);
    }
}
