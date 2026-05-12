use std::io::{self, Write};
use fnv::FnvHashMap;

/// DictEncoder uses a flat string pool to store variable-length, null-terminated strings.
/// The `id` returned is the direct byte offset in the string pool.
pub struct DictEncoder {
    lookup: FnvHashMap<Vec<u8>, u32>,
    pool: Vec<u8>,
}

impl DictEncoder {
    pub fn len(&self) -> usize {
        self.lookup.len()
    }

    pub fn pool_size(&self) -> usize {
        self.pool.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lookup.is_empty()
    }

    pub fn new(string_capacity: usize, pool_byte_capacity: usize) -> Self {
        Self {
            lookup: FnvHashMap::with_capacity_and_hasher(string_capacity, Default::default()),
            pool: Vec::with_capacity(pool_byte_capacity), 
        }
    }

    pub fn insert(&mut self, string_bytes: &[u8]) -> u32 {
        if let Some(&offset) = self.lookup.get(string_bytes) {
            offset
        } else {
            let offset = self.pool.len() as u32;
            self.pool.extend_from_slice(string_bytes);
            self.pool.push(0); // Null terminator
            
            // Store a copy in the lookup table to map back to the offset
            self.lookup.insert(string_bytes.to_vec(), offset);
            offset
        }
    }

    pub fn get_string(&self, offset: u32) -> &str {
        let start = offset as usize;
        // Search forward for the null terminator
        let len = self.pool[start..].iter().position(|&b| b == 0).unwrap_or(0);
        let end = start + len;
        unsafe { std::str::from_utf8_unchecked(&self.pool[start..end]) }
    }
    
    pub fn write_to<W: Write>(&self, offset: u32, writer: &mut W) -> io::Result<()> {
        let start = offset as usize;
        let len = self.pool[start..].iter().position(|&b| b == 0).unwrap_or(0);
        let end = start + len;
        writer.write_all(&self.pool[start..end])
    }
}
