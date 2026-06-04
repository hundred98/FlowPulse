use super::PrintState;

pub struct PrintStateMachine {
    current_state: PrintState,
}

impl PrintStateMachine {
    pub fn new() -> Self {
        Self {
            current_state: PrintState::Idle,
        }
    }
    
    pub fn current_state(&self) -> PrintState {
        self.current_state
    }
    
    pub fn can_transition_to(&self, new_state: PrintState) -> bool {
        match (self.current_state, new_state) {
            (PrintState::Idle, PrintState::Starting) => true,
            (PrintState::Starting, PrintState::Printing) => true,
            (PrintState::Printing, PrintState::Paused) => true,
            (PrintState::Printing, PrintState::Stopping) => true,
            (PrintState::Printing, PrintState::Completed) => true,
            (PrintState::Printing, PrintState::Failed) => true,
            (PrintState::Paused, PrintState::Resuming) => true,
            (PrintState::Paused, PrintState::Stopping) => true,
            (PrintState::Resuming, PrintState::Printing) => true,
            (PrintState::Stopping, PrintState::Idle) => true,
            _ => false,
        }
    }
    
    pub fn transition_to(&mut self, new_state: PrintState) -> Result<(), String> {
        if self.can_transition_to(new_state) {
            self.current_state = new_state;
            Ok(())
        } else {
            Err(format!(
                "Invalid transition from {:?} to {:?}",
                self.current_state, new_state
            ))
        }
    }
}

impl Default for PrintStateMachine {
    fn default() -> Self {
        Self::new()
    }
}
