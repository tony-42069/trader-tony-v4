use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};

// Renamed from get_main_keyboard to match usage in commands.rs
pub fn main_menu() -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new(vec![ // Use vec! instead of array literal
        vec![ // Row 1
            InlineKeyboardButton::callback("🤖 AutoTrader", "autotrader_menu"),
            InlineKeyboardButton::callback("📊 Positions", "positions_menu"),
        ],
        vec![ // Row 2
            InlineKeyboardButton::callback("💰 Balance", "show_balance"),
            InlineKeyboardButton::callback("⚙️ Strategies", "strategy_menu"),
        ],
        vec![ // Row 3 (Single button)
            InlineKeyboardButton::callback("❓ Help", "show_help"),
        ],
    ])
}

// Renamed from get_autotrader_keyboard
pub fn autotrader_menu(is_running: bool) -> InlineKeyboardMarkup {
    if is_running {
        InlineKeyboardMarkup::new(vec![ // Use vec!
            vec![ // Row 1
                InlineKeyboardButton::callback("⏹️ Stop", "stop_autotrader"),
                InlineKeyboardButton::callback("📊 Performance", "autotrader_performance"),
            ],
            vec![ // Row 2
                InlineKeyboardButton::callback("⚙️ Strategies", "strategy_menu"),
                InlineKeyboardButton::callback("🔙 Back", "main_menu"),
            ],
        ])
    } else {
        InlineKeyboardMarkup::new(vec![ // Use vec!
            vec![ // Row 1
                InlineKeyboardButton::callback("▶️ Start", "start_autotrader"),
                InlineKeyboardButton::callback("⚙️ Strategies", "strategy_menu"),
             ],
             vec![ // Row 2 (Single button)
                InlineKeyboardButton::callback("🔙 Back", "main_menu"),
             ],
        ])
    }
}

// Renamed from get_strategies_keyboard
pub fn strategy_menu() -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new(vec![
        vec![InlineKeyboardButton::callback("📋 List Strategies", "strategy_list")],
        vec![InlineKeyboardButton::callback("➕ Add Strategy", "strategy_add")],
        vec![InlineKeyboardButton::callback("🏠 Back to Main Menu", "show_main_menu")],
    ])
}

// Added placeholder for positions_menu used in commands.rs
pub fn positions_menu() -> InlineKeyboardMarkup {
     InlineKeyboardMarkup::new(vec![ // Use vec!
        vec![ // Row 1
            InlineKeyboardButton::callback("🔄 Refresh Positions", "refresh_positions"),
            // Add buttons for closing positions?
        ],
        vec![ // Row 2
            InlineKeyboardButton::callback("🔙 Back", "main_menu"),
        ],
    ])
}

/// Strategy List menu - shown when viewing all strategies
pub fn strategy_list_menu() -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new(vec![
        vec![InlineKeyboardButton::callback("➕ Add New Strategy", "strategy_add")],
        vec![InlineKeyboardButton::callback("🏠 Back to Main Menu", "show_main_menu")],
    ])
}

/// Strategy Detail menu - shown when viewing a specific strategy
pub fn strategy_detail_menu(strategy_id: &str, is_enabled: bool) -> InlineKeyboardMarkup {
    let toggle_text = if is_enabled { "🔴 Disable" } else { "✅ Enable" };
    let toggle_callback = format!("strategy_toggle:{}", strategy_id);
    let edit_callback = format!("strategy_edit:{}", strategy_id);
    let delete_callback = format!("strategy_delete:{}", strategy_id);

    InlineKeyboardMarkup::new(vec![ // Use vec!
        vec![ // Row 1
            InlineKeyboardButton::callback(toggle_text, toggle_callback),
            InlineKeyboardButton::callback("✏️ Edit", edit_callback),
        ],
        vec![ // Row 2
            InlineKeyboardButton::callback("🗑️ Delete", delete_callback),
            InlineKeyboardButton::callback("🔙 Back to Strategies", "strategy_menu"),
        ],
    ])
}

pub fn risk_levels_menu() -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new(vec![ // Use vec!
        vec![ // Row 1
            InlineKeyboardButton::callback("🟢 Low (<=30)", "set_risk:30"),
            InlineKeyboardButton::callback("🟠 Medium (<=50)", "set_risk:50"),
        ],
        vec![ // Row 2
            InlineKeyboardButton::callback("🔴 High (<=70)", "set_risk:70"),
            InlineKeyboardButton::callback("⚫ Custom", "set_risk:custom"),
        ],
         vec![ // Row 3 (Single button)
            InlineKeyboardButton::callback("❌ Cancel", "cancel_risk_setting")
         ],
    ])
}

pub fn position_size_menu() -> InlineKeyboardMarkup {
    InlineKeyboardMarkup::new(vec![ // Use vec!
        vec![ // Row 1
            InlineKeyboardButton::callback("0.01 SOL", "set_pos_size:0.01"),
            InlineKeyboardButton::callback("0.05 SOL", "set_pos_size:0.05"),
        ],
        vec![ // Row 2
            InlineKeyboardButton::callback("0.1 SOL", "set_pos_size:0.1"),
            InlineKeyboardButton::callback("0.5 SOL", "set_pos_size:0.5"),
        ],
        vec![ // Row 3 (Single button)
            InlineKeyboardButton::callback("⚫ Custom", "set_pos_size:custom"),
        ],
        vec![ // Row 4 (Single button)
            InlineKeyboardButton::callback("❌ Cancel", "cancel_pos_size_setting"),
        ],
    ])
}

// Generic confirmation keyboard
pub fn confirmation_menu(action_tag: &str, context: &str) -> InlineKeyboardMarkup {
    let confirm_callback = format!("confirm:{}:{}", action_tag, context);
    let cancel_callback = format!("cancel:{}:{}", action_tag, context);

    InlineKeyboardMarkup::new(vec![ // Use vec!
        vec![ // Row 1
            InlineKeyboardButton::callback("✅ Yes, Confirm", confirm_callback),
            InlineKeyboardButton::callback("❌ No, Cancel", cancel_callback),
        ],
    ])
}

// Token action menu for analyzed tokens
pub fn token_action_menu(token_address: &str) -> InlineKeyboardMarkup {
    let snipe_callback = format!("snipe_token:{}", token_address);
    let analyze_again_callback = format!("analyze_token:{}", token_address);

    InlineKeyboardMarkup::new(vec![
        vec![
            InlineKeyboardButton::callback("🎯 Snipe This Token", snipe_callback),
        ],
        vec![
            InlineKeyboardButton::callback("🔄 Analyze Again", analyze_again_callback),
            InlineKeyboardButton::callback("🔙 Back", "main_menu"),
        ],
    ])
}
