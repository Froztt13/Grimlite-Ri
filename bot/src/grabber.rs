//! Grimlite-Li compatible Loader / Grabber data model and Flash/State fetcher.

use crate::Bot;
use crate::host::FlashArg;
use serde_json::Value;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GrabKind {
    ShopItems,
    QuestIds,
    Quests,
    InventoryItems,
    TempItems,
    BankItems,
    Monsters,
    AllMonsters,
}

impl GrabKind {
    pub const ALL: &[Self] = &[
        Self::ShopItems,
        Self::QuestIds,
        Self::Quests,
        Self::InventoryItems,
        Self::TempItems,
        Self::BankItems,
        Self::Monsters,
        Self::AllMonsters,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::ShopItems => "Shop items",
            Self::QuestIds => "Quest IDs",
            Self::Quests => "Quest items, drop rates",
            Self::InventoryItems => "Inventory items",
            Self::TempItems => "Temp inventory items",
            Self::BankItems => "Bank items",
            Self::Monsters => "Monsters",
            Self::AllMonsters => "All Monsters",
        }
    }

    pub fn supports_order_by(self) -> bool {
        matches!(
            self,
            Self::QuestIds | Self::Quests | Self::Monsters | Self::AllMonsters
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GrabOrderBy {
    Name,
    Id,
}

impl GrabOrderBy {
    pub const ALL: &[Self] = &[Self::Name, Self::Id];

    pub fn label(self) -> &'static str {
        match self {
            Self::Name => "Name",
            Self::Id => "Id",
        }
    }
}

#[derive(Clone, Debug)]
pub struct GrabItemMeta {
    pub name: String,
    pub qty: i32,
    pub is_temp: bool,
}

/// Context-menu action metadata attached to a grabbed tree node.
#[derive(Clone, Debug)]
pub enum GrabNodeAction {
    None,
    WikiCopy {
        name: String,
    },
    Shop {
        shop_id: i32,
        name: String,
    },
    ShopItem {
        shop_id: i32,
        item_id: i32,
        shop_item_id: i32,
        name: String,
        qty: i32,
        cost: i32,
        is_ac: bool,
    },
    Quest {
        quest_id: i32,
        i_slot: i32,
        i_value: i32,
        reqs: Vec<GrabItemMeta>,
    },
    ItemList {
        items: Vec<GrabItemMeta>,
    },
    Item {
        name: String,
        is_temp: bool,
    },
}

#[derive(Clone, Debug)]
pub struct GrabNode {
    pub label: String,
    pub action: GrabNodeAction,
    pub children: Vec<GrabNode>,
}

impl GrabNode {
    fn leaf(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            action: GrabNodeAction::None,
            children: Vec::new(),
        }
    }
}

fn shorten(s: &str, max: usize) -> String {
    let trimmed = s.trim();
    if trimmed.chars().count() <= max {
        trimmed.to_string()
    } else {
        let mut out: String = trimmed.chars().take(max).collect();
        out.push_str("...");
        out
    }
}

fn val_str(v: &Value, keys: &[&str]) -> String {
    for &k in keys {
        if let Some(val) = v.get(k) {
            if let Some(s) = val.as_str() {
                return s.to_string();
            }
            if let Some(n) = val.as_i64() {
                return n.to_string();
            }
        }
    }
    String::new()
}

fn val_i32(v: &Value, keys: &[&str]) -> i32 {
    for &k in keys {
        if let Some(val) = v.get(k) {
            if let Some(n) = val.as_i64() {
                return n as i32;
            }
            if let Some(s) = val.as_str()
                && let Ok(n) = s.trim().parse::<i32>()
            {
                return n;
            }
        }
    }
    0
}

fn val_bool(v: &Value, keys: &[&str]) -> bool {
    for &k in keys {
        if let Some(val) = v.get(k) {
            if let Some(b) = val.as_bool() {
                return b;
            }
            if let Some(n) = val.as_i64() {
                return n != 0;
            }
            if let Some(s) = val.as_str() {
                return s == "1" || s.eq_ignore_ascii_case("true");
            }
        }
    }
    false
}

/// Fetches grabbed tree nodes from Flash (`getGameObject` / `GetMonstersInCell`),
/// falling back to `bot.state` where applicable. Blocking — run on a background thread.
pub fn fetch_grabbed(bot: &Arc<Bot>, kind: GrabKind, order: GrabOrderBy) -> Vec<GrabNode> {
    match kind {
        GrabKind::ShopItems => grab_shop_items(bot),
        GrabKind::QuestIds => grab_quests(bot, order, false),
        GrabKind::Quests => grab_quests(bot, order, true),
        GrabKind::InventoryItems => grab_items(bot, true),
        GrabKind::TempItems => grab_temp_items(bot),
        GrabKind::BankItems => grab_items(bot, false),
        GrabKind::Monsters => grab_monsters(bot, order, false),
        GrabKind::AllMonsters => grab_monsters(bot, order, true),
    }
}

fn get_game_json(bot: &Arc<Bot>, path: &str) -> Option<Value> {
    let flash = bot.flash()?;
    let raw = flash
        .call_str("getGameObject", &[FlashArg::from(path)])
        .ok()?;
    serde_json::from_str::<Value>(&raw).ok()
}

fn grab_shop_items(bot: &Arc<Bot>) -> Vec<GrabNode> {
    let Some(shop) = get_game_json(bot, "world.shopinfo") else {
        return Vec::new();
    };
    let shop_id = val_i32(&shop, &["ShopID", "id"]);
    let shop_name = val_str(&shop, &["sName", "name"]);
    if shop_id == 0 && shop_name.is_empty() {
        return Vec::new();
    }
    let location = val_str(&shop, &["sLocation"]);
    let mut children = vec![
        GrabNode::leaf(format!("ID: {shop_id}")),
        GrabNode::leaf(format!("Location: {location}")),
    ];

    if let Some(items) = shop.get("items").and_then(Value::as_array) {
        let mut item_nodes = Vec::new();
        for it in items {
            let name = val_str(it, &["sName", "Name"]);
            if name.is_empty() {
                continue;
            }
            let item_id = val_i32(it, &["ItemID", "id"]);
            let shop_item_id = val_i32(it, &["ShopItemID"]);
            let cost = val_i32(it, &["iCost"]);
            let is_ac = val_bool(it, &["bCoins"]);
            let cat = val_str(it, &["sType"]);
            let lvl = val_i32(it, &["iLvl"]);
            let desc = val_str(it, &["sDesc"]);
            let qty = val_i32(it, &["iQty"]).max(1);
            let max_stk = val_i32(it, &["iStk"]).max(1);
            let label = if qty > 1 {
                format!("{name} x{qty}")
            } else {
                name.clone()
            };
            let mut details = vec![
                GrabNode::leaf(format!("Shop item ID: {shop_item_id}")),
                GrabNode::leaf(format!("ID: {item_id}")),
                GrabNode::leaf(format!(
                    "Cost: {cost} {}",
                    if is_ac { "AC" } else { "Gold" }
                )),
                GrabNode::leaf(format!("Category: {cat}")),
                GrabNode::leaf(format!("Level: {lvl}")),
                GrabNode::leaf(format!("Stack: {qty}/{max_stk}")),
            ];
            if !desc.is_empty() {
                details.push(GrabNode::leaf(format!("Description: {}", shorten(&desc, 36))));
            }
            item_nodes.push(GrabNode {
                label,
                action: GrabNodeAction::ShopItem {
                    shop_id,
                    item_id,
                    shop_item_id,
                    name,
                    qty,
                    cost,
                    is_ac,
                },
                children: details,
            });
        }
        if !item_nodes.is_empty() {
            children.push(GrabNode {
                label: format!("Items ({})", item_nodes.len()),
                action: GrabNodeAction::None,
                children: item_nodes,
            });
        }
    }

    vec![GrabNode {
        label: if shop_name.is_empty() {
            format!("Shop #{shop_id}")
        } else {
            shop_name.clone()
        },
        action: GrabNodeAction::Shop {
            shop_id,
            name: shop_name,
        },
        children,
    }]
}

fn grab_quests(bot: &Arc<Bot>, order: GrabOrderBy, detailed: bool) -> Vec<GrabNode> {
    let Some(tree) = get_game_json(bot, "world.questTree") else {
        return Vec::new();
    };
    let Some(map) = tree.as_object() else {
        return Vec::new();
    };

    let mut quests: Vec<(i32, String, &Value)> = map
        .values()
        .filter_map(|q| {
            let id = val_i32(q, &["QuestID", "id"]);
            if id == 0 {
                return None;
            }
            let name = val_str(q, &["sName", "name"]);
            Some((id, name, q))
        })
        .collect();

    match order {
        GrabOrderBy::Name => quests.sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase())),
        GrabOrderBy::Id => quests.sort_by_key(|a| a.0),
    }

    quests
        .into_iter()
        .map(|(id, name, q)| {
            let i_slot = val_i32(q, &["iSlot"]);
            let i_value = val_i32(q, &["iValue"]);
            let label = match order {
                GrabOrderBy::Id => format!("{id} - {name}"),
                GrabOrderBy::Name => format!("{name} - {id}"),
            };

            let mut req_metas = Vec::new();
            let mut req_nodes = Vec::new();
            if let Some(turnins) = q.get("turnin").and_then(Value::as_array) {
                let o_items = q.get("oItems").and_then(Value::as_object);
                for req in turnins {
                    let item_id = val_i32(req, &["ItemID"]);
                    let qty = val_i32(req, &["iQty"]).max(1);
                    let meta = o_items.and_then(|m| m.get(&item_id.to_string()));
                    let r_name = meta
                        .map(|m| val_str(m, &["sName"]))
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| format!("Item #{item_id}"));
                    let is_temp = meta.map(|m| val_bool(m, &["bTemp"])).unwrap_or(false);
                    let r_desc = meta.map(|m| val_str(m, &["sDesc"])).unwrap_or_default();

                    req_metas.push(GrabItemMeta {
                        name: r_name.clone(),
                        qty,
                        is_temp,
                    });

                    if detailed {
                        let mut details = vec![
                            GrabNode::leaf(format!("ID: {item_id}")),
                            GrabNode::leaf(format!("Temporary: {}", if is_temp { "Yes" } else { "No" })),
                        ];
                        if !r_desc.is_empty() {
                            details.push(GrabNode::leaf(format!(
                                "Description: {}",
                                shorten(&r_desc, 36)
                            )));
                        }
                        req_nodes.push(GrabNode {
                            label: if qty > 1 {
                                format!("{r_name} x{qty}")
                            } else {
                                r_name.clone()
                            },
                            action: GrabNodeAction::Item {
                                name: r_name,
                                is_temp,
                            },
                            children: details,
                        });
                    }
                }
            }

            let mut children = Vec::new();
            if detailed {
                if i_slot > 0 {
                    children.push(GrabNode::leaf(format!("iSlot: {i_slot}")));
                }
                if i_value > 0 {
                    children.push(GrabNode::leaf(format!("iValue: {i_value}")));
                }
                let desc = val_str(q, &["sDesc"]);
                if !desc.is_empty() {
                    children.push(GrabNode::leaf(format!("Description: {}", shorten(&desc, 36))));
                }
                if !req_nodes.is_empty() {
                    children.push(GrabNode {
                        label: "Required items".into(),
                        action: GrabNodeAction::ItemList {
                            items: req_metas.clone(),
                        },
                        children: req_nodes,
                    });
                }

                // Rewards
                let mut rew_metas = Vec::new();
                let mut rew_nodes = Vec::new();
                if let Some(rewards) = q.get("reward").and_then(Value::as_array) {
                    let o_rewards = q.get("oRewards").and_then(Value::as_object);
                    for rw in rewards {
                        let item_id = val_i32(rw, &["ItemID"]);
                        let qty = val_i32(rw, &["iQty"]).max(1);
                        let rate = val_str(rw, &["iRate"]);
                        // Find reward metadata in oRewards (nested by type or flat)
                        let meta = find_reward_meta(o_rewards, item_id);
                        let rw_name = meta
                            .map(|m| val_str(m, &["sName"]))
                            .filter(|s| !s.is_empty())
                            .unwrap_or_else(|| format!("Item #{item_id}"));
                        let cat = meta.map(|m| val_str(m, &["sType"])).unwrap_or_default();
                        let rw_desc = meta.map(|m| val_str(m, &["sDesc"])).unwrap_or_default();
                        let drop_str = if rate.contains("100") {
                            "Guaranteed".to_string()
                        } else if rate.is_empty() {
                            "?".to_string()
                        } else {
                            format!("{rate}%")
                        };

                        rew_metas.push(GrabItemMeta {
                            name: rw_name.clone(),
                            qty,
                            is_temp: false,
                        });

                        let mut details = vec![
                            GrabNode::leaf(format!("ID: {item_id}")),
                            GrabNode::leaf(format!("Drop chance: {drop_str}")),
                        ];
                        if !cat.is_empty() {
                            details.push(GrabNode::leaf(format!("Category: {cat}")));
                        }
                        if !rw_desc.is_empty() {
                            details.push(GrabNode::leaf(format!(
                                "Description: {}",
                                shorten(&rw_desc, 36)
                            )));
                        }
                        rew_nodes.push(GrabNode {
                            label: if qty > 1 {
                                format!("{rw_name} x{qty}")
                            } else {
                                rw_name.clone()
                            },
                            action: GrabNodeAction::Item {
                                name: rw_name,
                                is_temp: false,
                            },
                            children: details,
                        });
                    }
                }
                if !rew_nodes.is_empty() {
                    children.push(GrabNode {
                        label: "Rewards".into(),
                        action: GrabNodeAction::ItemList { items: rew_metas },
                        children: rew_nodes,
                    });
                }
            }

            GrabNode {
                label,
                action: GrabNodeAction::Quest {
                    quest_id: id,
                    i_slot,
                    i_value,
                    reqs: req_metas,
                },
                children,
            }
        })
        .collect()
}

fn find_reward_meta<'a>(
    o_rewards: Option<&'a serde_json::Map<String, Value>>,
    item_id: i32,
) -> Option<&'a Value> {
    let map = o_rewards?;
    let key = item_id.to_string();
    if let Some(v) = map.get(&key) {
        return Some(v);
    }
    for sub in map.values() {
        if let Some(obj) = sub.as_object()
            && let Some(v) = obj.get(&key)
        {
            return Some(v);
        }
    }
    None
}

fn grab_items(bot: &Arc<Bot>, inventory: bool) -> Vec<GrabNode> {
    let path = if inventory {
        "world.myAvatar.items"
    } else {
        "world.bankinfo.items"
    };
    if let Some(val) = get_game_json(bot, path)
        && let Some(arr) = val.as_array()
    {
        let mut items: Vec<&Value> = arr.iter().collect();
        items.sort_by(|a, b| {
            val_str(a, &["sName"])
                .to_lowercase()
                .cmp(&val_str(b, &["sName"]).to_lowercase())
        });
        return items
            .into_iter()
            .filter_map(|it| {
                let name = val_str(it, &["sName"]);
                if name.is_empty() {
                    return None;
                }
                let id = val_i32(it, &["ItemID"]);
                let char_id = val_i32(it, &["CharItemID"]);
                let qty = val_i32(it, &["iQty"]).max(1);
                let max_stk = val_i32(it, &["iStk"]).max(1);
                let is_ac = val_bool(it, &["bCoins"]);
                let cat = val_str(it, &["sType"]);
                let lvl = val_i32(it, &["iLvl"]);
                let desc = val_str(it, &["sDesc"]);

                let mut details = vec![
                    GrabNode::leaf(format!("ID: {id}")),
                    GrabNode::leaf(format!("Char item ID: {char_id}")),
                    GrabNode::leaf(format!("Quantity: {qty}/{max_stk}")),
                    GrabNode::leaf(format!("AC tagged: {is_ac}")),
                    GrabNode::leaf(format!("Category: {cat}")),
                    GrabNode::leaf(format!("Level: {lvl}")),
                ];
                if !desc.is_empty() {
                    details.push(GrabNode::leaf(format!("Description: {}", shorten(&desc, 36))));
                }
                Some(GrabNode {
                    label: format!("{name} ({id})"),
                    action: GrabNodeAction::Item {
                        name,
                        is_temp: false,
                    },
                    children: details,
                })
            })
            .collect();
    }

    // Fallback to live packet state (Headless mode)
    let mut items = if inventory {
        bot.state.inventory.read().clone()
    } else {
        bot.state.bank.read().clone()
    };
    items.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    items
        .into_iter()
        .map(|it| GrabNode {
            label: format!("{} ({})", it.name, it.id),
            action: GrabNodeAction::Item {
                name: it.name,
                is_temp: false,
            },
            children: vec![
                GrabNode::leaf(format!("ID: {}", it.id)),
                GrabNode::leaf(format!("Char item ID: {}", it.char_item_id)),
                GrabNode::leaf(format!("Quantity: {}", it.qty)),
                GrabNode::leaf(format!("AC tagged: {}", it.is_ac)),
                GrabNode::leaf(format!("Category: {}", it.category)),
            ],
        })
        .collect()
}

fn grab_temp_items(bot: &Arc<Bot>) -> Vec<GrabNode> {
    if let Some(val) = get_game_json(bot, "world.myAvatar.tempitems") {
        let mut raw_items: Vec<&Value> = match &val {
            Value::Array(a) => a.iter().collect(),
            Value::Object(m) => m.values().collect(),
            _ => Vec::new(),
        };
        raw_items.sort_by(|a, b| {
            val_str(a, &["sName"])
                .to_lowercase()
                .cmp(&val_str(b, &["sName"]).to_lowercase())
        });
        return raw_items
            .into_iter()
            .filter_map(|it| {
                let name = val_str(it, &["sName"]);
                if name.is_empty() {
                    return None;
                }
                let id = val_i32(it, &["ItemID"]);
                let qty = val_i32(it, &["iQty"]);
                let max_stk = val_i32(it, &["iStk"]).max(1);
                Some(GrabNode {
                    label: format!("{name} [{qty}/{max_stk}]"),
                    action: GrabNodeAction::WikiCopy { name },
                    children: vec![
                        GrabNode::leaf(format!("ID: {id}")),
                        GrabNode::leaf(format!("Quantity: {qty}/{max_stk}")),
                    ],
                })
            })
            .collect();
    }

    let mut items = bot.state.temp_inventory.read().clone();
    items.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    items
        .into_iter()
        .map(|it| GrabNode {
            label: format!("{} [{}]", it.name, it.qty),
            action: GrabNodeAction::WikiCopy { name: it.name },
            children: vec![
                GrabNode::leaf(format!("ID: {}", it.id)),
                GrabNode::leaf(format!("Quantity: {}", it.qty)),
            ],
        })
        .collect()
}

fn grab_monsters(bot: &Arc<Bot>, order: GrabOrderBy, all_in_map: bool) -> Vec<GrabNode> {
    let mut mons: Vec<(i32, i32, String, String, String, i32, i32, i32)> = Vec::new();

    if !all_in_map
        && let Some(flash) = bot.flash()
        && let Ok(raw) = flash.call_str("GetMonstersInCell", &[])
        && let Ok(Value::Array(arr)) = serde_json::from_str::<Value>(&raw)
    {
        for m in &arr {
            let name = val_str(m, &["strMonName", "Name"]);
            if name.is_empty() {
                continue;
            }
            mons.push((
                val_i32(m, &["MonID", "id"]),
                val_i32(m, &["MonMapID"]),
                name,
                val_str(m, &["strFrame", "cell"]),
                val_str(m, &["sRace"]),
                val_i32(m, &["intLevel"]),
                val_i32(m, &["intHP"]),
                val_i32(m, &["intHPMax"]),
            ));
        }
    } else if all_in_map
        && let Some(val) = get_game_json(bot, "world.monsters")
    {
        let iter: Vec<&Value> = match &val {
            Value::Array(a) => a.iter().collect(),
            Value::Object(m) => m.values().collect(),
            _ => Vec::new(),
        };
        for m in iter {
            let data = m.get("objData").unwrap_or(m);
            let name = val_str(data, &["strMonName", "Name"]);
            if name.is_empty() {
                continue;
            }
            mons.push((
                val_i32(data, &["MonID", "id"]),
                val_i32(m, &["MonMapID"]),
                name,
                val_str(m, &["strFrame"]),
                val_str(data, &["sRace"]),
                val_i32(data, &["intLevel"]),
                val_i32(m, &["intHP"]),
                val_i32(m, &["intHPMax"]),
            ));
        }
    }

    // Fallback to live packet state
    if mons.is_empty() {
        let cur_cell = bot.player_snapshot().cell;
        for m in bot.state.monsters.read().iter() {
            if !all_in_map && !cur_cell.is_empty() && !m.cell.eq_ignore_ascii_case(&cur_cell) {
                continue;
            }
            mons.push((
                0,
                m.mon_map_id,
                m.name.clone(),
                m.cell.clone(),
                String::new(),
                0,
                m.hp,
                m.max_hp,
            ));
        }
    }

    match order {
        GrabOrderBy::Name => mons.sort_by(|a, b| a.2.to_lowercase().cmp(&b.2.to_lowercase())),
        GrabOrderBy::Id => mons.sort_by_key(|a| a.1),
    }

    mons.into_iter()
        .map(|(mon_id, mon_map_id, name, cell, race, lvl, hp, max_hp)| {
            let label = if all_in_map && !cell.is_empty() {
                format!("{name} ({cell})")
            } else {
                name.clone()
            };
            let mut details = vec![
                GrabNode::leaf(format!("ID: {mon_id}")),
                GrabNode::leaf(format!("MonMapID: {mon_map_id}")),
            ];
            if !race.is_empty() {
                details.push(GrabNode::leaf(format!("Race: {race}")));
            }
            if lvl > 0 {
                details.push(GrabNode::leaf(format!("Level: {lvl}")));
            }
            details.push(GrabNode::leaf(format!("Health: {hp}/{max_hp}")));
            GrabNode {
                label,
                action: GrabNodeAction::WikiCopy { name },
                children: details,
            }
        })
        .collect()
}
