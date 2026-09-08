mod helpers;
mod policy;
use compact::CompactTabActionsMenuState;
use helpers::{
    TAB_ACTIONS_MENU_WIDTH, clamp_tab_actions_position, tab_actions_menu_visible_height,
};
use policy::{TabActionMenuGroup, TabActionPolicy, TabActionPolicyInput, TabSessionCapability};

mod compact;
mod overlay;
