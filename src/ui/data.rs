//! The maps and the catalog, read from the game's own assets.

use crate::assets::{db::AssetDb, mono::Value};
use std::path::Path;

pub struct MapEntry {
    pub name: String,
    pub description: String,
}

pub struct Category {
    pub name: String,
    pub description: String,
}

pub struct Item {
    pub name: String,
    pub description: String,
    pub category: usize,
    /// NameToOrderBy, or the name.
    pub order: String,
}

#[derive(Default)]
pub struct Catalog {
    pub categories: Vec<Category>,
    pub items: Vec<Item>,
}

fn string(v: &Value, field: &str) -> String {
    match v.get(field) {
        Some(Value::Str(s)) => s.clone(),
        _ => String::new(),
    }
}

/// The Map assets in registry order: what the map selection menu lists.
pub fn maps(game: &Path) -> Result<Vec<MapEntry>, String> {
    let mut db = AssetDb::open(game)?;
    let mut out = Vec::new();
    for m in db.find_scripts("Map")? {
        let Some((_, mb, v)) = db.mono(m)? else { continue };
        out.push(MapEntry { name: mb.name, description: string(&v, "Description") });
    }
    Ok(out)
}

/// CatalogData: the categories in order and the items the catalog shows (VisibleInCatalog, not Locked), each in its
/// category, in CatalogBehaviour's order (NameToOrderBy when set, else the name).
pub fn catalog(db: &mut AssetDb) -> Result<Catalog, String> {
    let data = *db.find_scripts("CatalogData")?.first().ok_or("no CatalogData")?;
    let (_, _, root) = db.mono(data)?.ok_or("CatalogData")?;
    let mut catalog = Catalog::default();
    let mut category_ids = Vec::new();
    for c in root.get("Categories").and_then(Value::array).ok_or("CatalogData.Categories")? {
        let Some(r) = c.pptr().and_then(|p| db.resolve(data.file, p)) else { continue };
        let (_, mb, v) = db.mono(r)?.ok_or("Category")?;
        category_ids.push(r);
        catalog.categories.push(Category { name: mb.name, description: string(&v, "Description") });
    }
    for item in root.get("Items").and_then(Value::array).ok_or("CatalogData.Items")? {
        let Some(r) = item.pptr().and_then(|p| db.resolve(data.file, p)) else { continue };
        let (_, mb, v) = db.mono(r)?.ok_or("SpawnableAsset")?;
        let visible = v.get("VisibleInCatalog").and_then(Value::bool).unwrap_or(true);
        let locked = v.get("Locked").and_then(Value::bool).unwrap_or(false);
        let category = v.get("Category").and_then(Value::pptr).and_then(|p| db.resolve(r.file, p)).and_then(|c| category_ids.iter().position(|&x| x == c));
        let (true, false, Some(category)) = (visible, locked, category) else { continue };
        let order = Some(string(&v, "NameToOrderBy")).filter(|s| !s.is_empty()).unwrap_or_else(|| mb.name.clone());
        catalog.items.push(Item { name: mb.name, description: string(&v, "Description"), category, order });
    }
    catalog.items.sort_by(|a, b| a.order.cmp(&b.order));
    Ok(catalog)
}

impl Catalog {
    /// The items to list: the filter (case-insensitive, against name and category) when set, else the category.
    pub fn shown(&self, category: usize, filter: &str) -> Vec<&Item> {
        let f = filter.to_lowercase();
        self.items
            .iter()
            .filter(|i| match f.is_empty() {
                true => i.category == category,
                false => format!("{}{}", i.name, self.categories[i.category].name).to_lowercase().contains(&f),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cat() -> Catalog {
        let item = |name: &str, category, order: &str| Item { name: name.into(), description: String::new(), category, order: order.into() };
        Catalog {
            categories: ["Biological", "Firearms"].map(|n| Category { name: n.into(), description: String::new() }).into(),
            items: vec![item("Human", 0, "Human"), item("Pistol", 1, "Pistol"), item("Rifle", 1, "Rifle")],
        }
    }

    #[test]
    fn shown_by_category_or_filter() {
        let c = cat();
        assert_eq!(c.shown(1, "").iter().map(|i| i.name.as_str()).collect::<Vec<_>>(), ["Pistol", "Rifle"]);
        assert_eq!(c.shown(0, "FIREARM").len(), 2);
        assert_eq!(c.shown(0, "pist").len(), 1);
    }

    /// Reads the installed game (cargo test -- --ignored).
    #[test]
    #[ignore]
    fn the_installed_game_lists_maps_and_a_catalog() {
        let game = crate::game_assets::find_game(None).unwrap();
        let maps = maps(&game).unwrap();
        assert!(maps.iter().any(|m| m.name == "Default"), "{:?}", maps.iter().map(|m| &m.name).collect::<Vec<_>>());
        let mut db = AssetDb::open(&game).unwrap();
        let c = catalog(&mut db).unwrap();
        println!("{} maps, {} categories, {} items", maps.len(), c.categories.len(), c.items.len());
        assert!(c.items.iter().any(|i| i.name == "Pistol"));
        assert!(!c.categories.is_empty());
    }
}
