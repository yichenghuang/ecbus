use std::io::{self, Write};
use fnv::FnvHashMap;

pub struct PlateManager {
    lookup: FnvHashMap<[u8; 8], u32>,
    pool: Vec<[u8; 8]>,
}

impl PlateManager {
    pub fn len(&self) -> usize {
        self.pool.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pool.is_empty()
    }

    pub fn new(capacity: usize) -> Self {
        Self {
            lookup: FnvHashMap::with_capacity_and_hasher(capacity, Default::default()),
            pool: Vec::with_capacity(capacity),
        }
    }

    pub fn insert(&mut self, plate_bytes: &[u8]) -> u32 {
        let mut fixed_plate = [0u8; 8];
        let len = plate_bytes.len().min(8);
        fixed_plate[..len].copy_from_slice(&plate_bytes[..len]);

        if let Some(&index) = self.lookup.get(&fixed_plate) {
            index
        } else {
            let new_index = self.pool.len() as u32;
            self.pool.push(fixed_plate);
            self.lookup.insert(fixed_plate, new_index);
            new_index
        }
    }

    pub fn get_string(&self, index: u32) -> &str {
        let bytes = &self.pool[index as usize];
        let len = bytes.iter().position(|&b| b == 0).unwrap_or(8);
        unsafe { std::str::from_utf8_unchecked(&bytes[..len]) }
    }
    
    pub fn write_to<W: Write>(&self, index: u32, writer: &mut W) -> io::Result<()> {
        let bytes = &self.pool[index as usize];        
        let len = bytes.iter().position(|&b| b == 0).unwrap_or(8);
        writer.write_all(&bytes[..len])
    }
}
