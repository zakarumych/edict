use core::hash::{Hash, Hasher};

// pub struct XorHasher {
//     value: u64,
// }

// impl XorHasher {
//     pub fn new() -> Self {
//         XorHasher { value: 0 }
//     }
// }

// impl Hasher for XorHasher {
//     fn write(&mut self, bytes: &[u8]) {
//         for chunk in bytes.chunks(8) {
//             let mut b = [0; 8];
//             b[..chunk.len()].copy_from_slice(chunk);
//             self.value ^= u64::from_ne_bytes(b);
//         }
//     }
//     fn write_u64(&mut self, i: u64) {
//         self.value ^= i;
//     }
//     fn write_u128(&mut self, i: u128) {
//         self.value ^= i as u64;
//     }
//     fn write_usize(&mut self, i: usize) {
//         self.value ^= i as u64;
//     }
//     fn finish(&self) -> u64 {
//         self.value
//     }
// }

pub struct NoOpHasher {
    value: u64,
}

impl NoOpHasher {
    pub fn new() -> Self {
        NoOpHasher { value: 0 }
    }
}

impl Hasher for NoOpHasher {
    fn write(&mut self, bytes: &[u8]) {
        let mut b = [0; 8];
        let c = 8.min(bytes.len());
        b[..c].copy_from_slice(&bytes[..c]);
        self.value = u64::from_ne_bytes(b);
    }
    fn write_u64(&mut self, i: u64) {
        self.value = i;
    }
    fn write_u128(&mut self, i: u128) {
        self.value = i as u64;
    }
    fn write_usize(&mut self, i: usize) {
        self.value = i as u64;
    }
    fn finish(&self) -> u64 {
        self.value
    }
}

// pub fn xor_hash<T>(v: &T) -> u64
// where
//     T: Hash,
// {
//     let mut hasher = XorHasher::new();
//     v.hash(&mut hasher);
//     hasher.finish()
// }

pub fn no_op_hash<T>(v: &T) -> u64
where
    T: Hash,
{
    let mut hasher = NoOpHasher::new();
    v.hash(&mut hasher);
    hasher.finish()
}
