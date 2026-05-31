//! Shared memory server for HMI communication

use std::sync::Mutex;

use super::types::{
    PrinterCmd, PrinterStatus, PrinterCommand,
    SharedMemory,
};

/// Shared memory server for HMI communication
pub struct SharedMemoryServer {
    data: Mutex<SharedMemory>,
    running: Mutex<bool>,
}

impl SharedMemoryServer {
    pub fn new() -> Self {
        Self {
            data: Mutex::new(SharedMemory::new()),
            running: Mutex::new(false),
        }
    }

    pub fn update_status(&self, status: PrinterStatus) {
        let mut data = self.data.lock().unwrap();
        data.status = status;
    }

    pub fn get_status(&self) -> PrinterStatus {
        let data = self.data.lock().unwrap();
        data.status.clone()
    }

    pub fn get_next_command(&self) -> Option<PrinterCommand> {
        let mut data = self.data.lock().unwrap();
        
        if data.cmd_queue.count == 0 {
            return None;
        }
        
        let cmd = data.cmd_queue.commands[data.cmd_queue.tail as usize].clone();
        data.cmd_queue.tail = (data.cmd_queue.tail + 1) % 32;
        data.cmd_queue.count -= 1;
        
        Some(cmd)
    }

    pub fn send_ack(&self, cmd: PrinterCmd, success: bool) {
        let mut data = self.data.lock().unwrap();
        
        let head = data.ack_queue.head;
        let next_head = (head + 1) % 32;
        
        if next_head != data.ack_queue.tail {
            data.ack_queue.commands[head as usize].cmd = if success { cmd } else { PrinterCmd::None };
            data.ack_queue.head = next_head;
            data.ack_queue.count += 1;
        }
    }

    pub fn start(&self) {
        let mut running = self.running.lock().unwrap();
        *running = true;
        println!("Shared memory server started");
    }

    pub fn stop(&self) {
        let mut running = self.running.lock().unwrap();
        *running = false;
        println!("Shared memory server stopped");
    }

    pub fn is_running(&self) -> bool {
        *self.running.lock().unwrap()
    }
    
    pub fn get_data(&self) -> std::sync::MutexGuard<'_, SharedMemory> {
        self.data.lock().unwrap()
    }
}

impl Default for SharedMemoryServer {
    fn default() -> Self {
        Self::new()
    }
}