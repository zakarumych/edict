/// Contiguous memory with specific layout.
pub struct Chunk {}

/// Collection of all entities with same set of components.
pub struct Archetype {
    offsets: Box<[usize]>,
}
