use std::error::Error;
use std::fs::File;
use csv::{ReaderBuilder, ByteRecord};
use crate::models::{Transaction, DayIndex, PlateIndex};
use crate::plate_manager::PlateManager;
use crate::dict_encoder::DictEncoder;
use crate::utils::{date_time_to_timestamp, logical_day_index};

pub fn easycard_loader(
    file_paths: &[&str], 
    pm: &mut PlateManager, 
    tx_type_dict: &mut DictEncoder, 
    company_dict: &mut DictEncoder
) -> Result<(Vec<Transaction>, Vec<DayIndex>), Box<dyn Error>> {
    let mut transactions = Vec::new();
    let mut record = ByteRecord::new();

    let mut last_logical_day_id: u64 = u64::MAX;
    let mut last_logical_date: i64 = 0;

    for &path in file_paths {
        let file = File::open(path)?;
        let mut rdr = ReaderBuilder::new()
            .has_headers(true)
            .from_reader(file);

        while rdr.read_byte_record(&mut record)? {
            // Optional: Skip if record is obviously too short
            if record.len() < 13 { continue; }

            let card_id_bytes = trim_bytes(&record[0]);
            let card_id: u64 = std::str::from_utf8(card_id_bytes).unwrap_or("0").parse().unwrap_or(0);

            let (seq_no_on, seq_no_off) = parse_u32_pair(&record[1]);
            let tx_type_id = tx_type_dict.insert(trim_bytes(&record[2]));

            let on_off_code_bytes = trim_bytes(&record[3]);
            let mut on_off_code = [0u8; 8];
            let len = on_off_code_bytes.len().min(8);
            on_off_code[..len].copy_from_slice(&on_off_code_bytes[..len]);
            
            let company_id = company_dict.insert(trim_bytes(&record[4]));

            let line_no_str = std::str::from_utf8(trim_bytes(&record[10])).unwrap_or("0");
            let line_no: u32 = line_no_str.parse().unwrap_or(0);

            let on_date_bytes = trim_bytes(&record[5]);
            let on_time_bytes = trim_bytes(&record[6]);
            
            if on_date_bytes.len() < 10 || on_time_bytes.len() < 8 {
                continue;
            }

            let on_time = date_time_to_timestamp(on_date_bytes, on_time_bytes);
            
            let logical_day_id = (on_time + 18000) / 86400; 

            let logical_day = if logical_day_id == last_logical_day_id {
                last_logical_date
            } else {
                let ld = logical_day_index(on_time);
                last_logical_day_id = logical_day_id;
                last_logical_date = ld;
                ld
            };

            let off_date_bytes = trim_bytes(&record[7]);
            let off_time_bytes = trim_bytes(&record[8]);
            let off_time = if !off_date_bytes.is_empty() && off_date_bytes != b"NULL" && !off_time_bytes.is_empty() && off_time_bytes != b"NULL" {
                date_time_to_timestamp(off_date_bytes, off_time_bytes)
            } else {
                0
            };

            let raw_plate = trim_bytes(&record[12]);
            let mut cleaned_plate = [0u8; 8];
            let mut len = 0;
            for &b in raw_plate {
                if b != b'-' && len < 8 {
                    cleaned_plate[len] = b;
                    len += 1;
                }
            }
            let plate_id = pm.insert(&cleaned_plate[..len]);

            transactions.push(Transaction {
                card_id,
                plate_id,
                on_time,
                off_time,
                logical_day,
                sorted_index: 0,
                seq_no_on,
                seq_no_off,
                on_off_code,
                tx_type_id,
                company_id,
                line_no,
            });
        }
    }

    transactions.sort_unstable_by(|a, b| {
        a.logical_day.cmp(&b.logical_day)
            .then_with(|| a.plate_id.cmp(&b.plate_id))
            .then_with(|| a.on_time.cmp(&b.on_time))
    });

    let mut day_indices = Vec::new();
    if !transactions.is_empty() {
        let mut current_day = transactions[0].logical_day;
        let mut start_idx = 0;
        
        for i in 0..transactions.len() {
            if transactions[i].logical_day != current_day {
                day_indices.push(DayIndex {
                    logical_day: current_day,
                    start: start_idx,
                    count: i - start_idx,
                });
                current_day = transactions[i].logical_day;
                start_idx = i;
            }
        }
        day_indices.push(DayIndex {
            logical_day: current_day,
            start: start_idx,
            count: transactions.len() - start_idx,
        });
    }

    Ok((transactions, day_indices))
}

pub fn build_plate_index(transactions: &[Transaction], plate_count: usize) -> Vec<PlateIndex> {
    let mut indices = vec![PlateIndex::default(); plate_count];
    for (i, t) in transactions.iter().enumerate() {
        let idx = &mut indices[t.plate_id as usize];
        if idx.count == 0 {
            idx.start = i;
        }
        idx.count += 1;
    }
    indices
}

fn trim_bytes(bytes: &[u8]) -> &[u8] {
    let mut start = 0;
    while start < bytes.len() && bytes[start].is_ascii_whitespace() {
        start += 1;
    }
    let mut end = bytes.len();
    while end > start && bytes[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    &bytes[start..end]
}

fn bytes_to_int_ignore_non_digit(bytes: &[u8]) -> u32 {
    let mut res = 0u32;
    for &b in bytes {
        if b >= b'0' && b <= b'9' {
            res = res * 10 + (b - b'0') as u32;
        }
    }
    res
}

fn parse_u32_pair(bytes: &[u8]) -> (u32, u32) {
    let mut parts = bytes.split(|&b| b == b',');
    let first_part = parts.next().unwrap_or(&[]);
    let second_part = parts.next().unwrap_or(&[]);

    let first = bytes_to_int_ignore_non_digit(first_part);
    let second = bytes_to_int_ignore_non_digit(second_part);
    (first, second)
}
