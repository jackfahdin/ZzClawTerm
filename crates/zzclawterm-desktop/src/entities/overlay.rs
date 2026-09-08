use gpui::Entity;
use zzclawterm_ui::ZzClawCommandState;

#[derive(Clone, Default)]
pub struct QuickSwitchState {
    command_state: Option<Entity<ZzClawCommandState>>,
}

impl QuickSwitchState {
    pub fn is_open(&self) -> bool {
        self.command_state.is_some()
    }

    pub fn command_state(&self) -> Option<Entity<ZzClawCommandState>> {
        self.command_state.clone()
    }
}

#[derive(Default)]
pub struct OverlayStore {
    quick_switch: QuickSwitchState,
}

impl OverlayStore {
    pub fn quick_switch(&self) -> &QuickSwitchState {
        &self.quick_switch
    }

    pub fn open_quick_switch(&mut self, command_state: Entity<ZzClawCommandState>) -> bool {
        self.quick_switch.command_state = Some(command_state);
        true
    }

    pub fn close_quick_switch(&mut self) -> bool {
        if self.quick_switch.command_state.take().is_none() {
            return false;
        }
        true
    }
}
