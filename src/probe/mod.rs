pub mod io;
pub mod target;
pub mod transport;

use target::dp::*;
use target::ap::*;
use transport::Transport;

pub struct Probe<T: Transport> {
    transport: T,
}

