// src/models.rs

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PairingStatus {
    Ok,
    NoArrival,
    NoDeparture,
}

impl Default for PairingStatus {
    fn default() -> Self {
        PairingStatus::Ok
    }
}

#[derive(Debug)]
pub struct Transaction {
    pub card_id: u64,
    pub plate_id: u32,
    pub on_time: u64,         // on timestamp
    pub off_time: u64,        // off timestamp
    pub logical_day: i64,     // epoch days shifted by 3 AM boundary
    pub sorted_index: u32,    // link to the index in the sorted array
    // New fields
    pub seq_no_on: u32,
    pub seq_no_off: u32,
    pub on_off_code: [u8; 8],
    pub tx_type_id: u32,
    pub company_id: u32,
    pub line_no: u32,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct DayIndex {
    pub logical_day: i64, 
    pub start: usize,
    pub count: usize,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PlateIndex {
    pub start: usize,
    pub count: usize,
}

#[derive(Debug)]
pub struct BusStop {
    pub plate_id: u32,
    pub stop_name_tw: u32,
    pub arrival_time: u64,
    pub depart_time: u64,
    pub pairing: PairingStatus,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct BusStopStats {
    pub raw_records: usize,
    pub final_records: usize,
    pub missing_arrival: usize,
    pub missing_depart: usize,
}
