pub struct Handle {}

impl Handle {
    pub fn send(self, id: u32) {}
}

pub struct Queue {}

impl Queue {
    pub fn make_handle(&self) -> Handle {}
    pub fn enqued(&mut self) -> impl Iterator<Item = u32> {}
}
