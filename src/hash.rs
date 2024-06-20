use core::hash::{BuildHasher, Hasher};

#[derive(Default)]
pub struct NoOpHasher {
    value: u64,
}

impl NoOpHasher {
    pub fn new() -> Self {
        NoOpHasher { value: 0 }
    }
}

impl Hasher for NoOpHasher {
    #[inline(always)]
    fn write(&mut self, bytes: &[u8]) {
        let mut b = [0; 8];
        let c = 8.min(bytes.len());
        b[..c].copy_from_slice(&bytes[..c]);
        self.value = u64::from_ne_bytes(b);
    }

    #[inline(always)]
    fn write_u64(&mut self, i: u64) {
        self.value = i;
    }

    #[inline(always)]
    fn write_u128(&mut self, i: u128) {
        self.value = i as u64;
    }

    #[inline(always)]
    fn write_usize(&mut self, i: usize) {
        self.value = i as u64;
    }

    #[inline(always)]
    fn finish(&self) -> u64 {
        self.value
    }
}

#[derive(Default)]
pub struct NoOpHasherBuilder;

impl BuildHasher for NoOpHasherBuilder {
    type Hasher = NoOpHasher;

    #[inline(always)]
    fn build_hasher(&self) -> NoOpHasher {
        NoOpHasher::new()
    }
}

const MUL_HASH_CONST_64: u64 = 11400714819323198485;

pub struct MulHasher {
    value: u64,
}

impl MulHasher {
    #[inline(always)]
    pub fn new() -> Self {
        MulHasher { value: 0 }
    }
}

impl Hasher for MulHasher {
    #[inline(always)]
    fn write(&mut self, bytes: &[u8]) {
        for chunk in bytes.chunks(8) {
            let mut b = [0; 8];
            b[..chunk.len()].copy_from_slice(chunk);
            self.value ^= u64::from_ne_bytes(b);
        }
    }

    #[inline(always)]
    fn write_u64(&mut self, i: u64) {
        self.value = self.value.wrapping_mul(MUL_HASH_CONST_64).wrapping_add(i);
    }

    #[inline(always)]
    fn write_u128(&mut self, i: u128) {
        self.write_u64(i as u64);
        self.write_u64((i >> 64) as u64);
    }

    #[inline(always)]
    fn write_usize(&mut self, i: usize) {
        self.write_u64(i as u64);
    }

    #[inline(always)]
    fn finish(&self) -> u64 {
        // Most significant bits are better than leas significant ones.
        self.value.wrapping_mul(MUL_HASH_CONST_64).swap_bytes()
    }
}

pub struct MulHasherBuilder;

impl BuildHasher for MulHasherBuilder {
    type Hasher = MulHasher;

    fn build_hasher(&self) -> MulHasher {
        MulHasher::new()
    }
}
