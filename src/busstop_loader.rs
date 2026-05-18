use std::error::Error;
use std::fs::File;
use csv::{ReaderBuilder, ByteRecord};
use chrono::DateTime;
use crate::models::{BusStop, PlateIndex, BusStopStats, PairingStatus};
use crate::plate_manager::PlateManager;
use crate::dict_encoder::DictEncoder;
use crate::utils::logical_day_index;

#[derive(Debug, PartialEq)]
enum EventType {
    Arrive,
    Depart,
}

struct RawGps {
    plate_id: u32,
    stop_name_tw: u32,
    event: EventType,
    time: u64,
}

struct RawStop {
    plate_id: u32,
    stop_name_tw: u32,
    arrival_time: Option<u64>,
    depart_time: Option<u64>,
    pairing: PairingStatus,
}

fn get_next_raw_stop(
    iter: &mut std::iter::Peekable<std::vec::IntoIter<RawGps>>,
    stats: &mut BusStopStats,
) -> Option<RawStop> {
    let first = iter.next()?;
    if first.event == EventType::Arrive {
        if let Some(next) = iter.peek() {
            if next.plate_id == first.plate_id && next.stop_name_tw == first.stop_name_tw && next.event == EventType::Depart {
                let depart_time = next.time;
                iter.next(); 
                
                return Some(RawStop { 
                    plate_id: first.plate_id, 
                    stop_name_tw: first.stop_name_tw, 
                    arrival_time: Some(first.time), 
                    depart_time: Some(depart_time),
                    pairing: PairingStatus::Ok,
                });
            }
        }
        stats.missing_depart += 1;
        Some(RawStop { 
            plate_id: first.plate_id, 
            stop_name_tw: first.stop_name_tw, 
            arrival_time: Some(first.time), 
            depart_time: None,
            pairing: PairingStatus::NoDeparture,
        })
    } else {
        stats.missing_arrival += 1;
        Some(RawStop { 
            plate_id: first.plate_id, 
            stop_name_tw: first.stop_name_tw, 
            arrival_time: None, 
            depart_time: Some(first.time),
            pairing: PairingStatus::NoArrival,
        })
    }
}

pub fn busstop_loader(
    file_paths: &[String], 
    target_logical_day: i64, 
    pm: &mut PlateManager, 
    dict: &mut DictEncoder
) -> Result<(Vec<BusStop>, BusStopStats), Box<dyn Error>> {
    let mut raw_records = Vec::new();
    let mut record = ByteRecord::new();

    for path in file_paths {
        let file = match File::open(path) {
            Ok(f) => f,
            Err(_) => continue,
        };
        
        let mut rdr = ReaderBuilder::new()
            .has_headers(true)
            .flexible(true) // Allow malformed/truncated rows
            .from_reader(file);

        let headers = match rdr.headers() {
            Ok(h) => h.clone(),
            Err(_) => continue,
        };
        if headers.get(0) != Some("PlateNumb") {
            continue;
        }

        while let Ok(has_more) = rdr.read_byte_record(&mut record) {
            if !has_more { break; }
            
            // Check if record is long enough to have the time string
            if record.len() < 22 { continue; }

            let time_str = std::str::from_utf8(record.get(21).unwrap_or(b"")).unwrap_or("").trim();
            let dt = match DateTime::parse_from_rfc3339(time_str) {
                Ok(d) => d,
                Err(_) => continue,
            };
            let time = dt.timestamp() as u64;
            let logical_day = logical_day_index(time);

            if logical_day < target_logical_day {
                continue;
            } else if logical_day > target_logical_day {
                break; 
            }

            let bus_status = std::str::from_utf8(record.get(19).unwrap_or(b""))?;
            if bus_status != "正常" {
                continue;
            }

            let plate_bytes = &record[0];
            let mut cleaned_plate = [0u8; 8];
            let mut len = 0;
            
            // Trim whitespace manually or use a helper
            let mut start = 0;
            while start < plate_bytes.len() && plate_bytes[start].is_ascii_whitespace() {
                start += 1;
            }
            let mut end = plate_bytes.len();
            while end > start && plate_bytes[end - 1].is_ascii_whitespace() {
                end -= 1;
            }

            for &b in &plate_bytes[start..end] {
                if b != b'-' && len < 8 {
                    cleaned_plate[len] = b;
                    len += 1;
                }
            }
            let plate_id = pm.insert(&cleaned_plate[..len]);

            let stop_name_str = std::str::from_utf8(&record[14])?.trim();
            let stop_name_tw = dict.insert(stop_name_str.as_bytes());

            let event_str = std::str::from_utf8(record.get(20).unwrap_or(b""))?;
            let event = if event_str == "進站" {
                EventType::Arrive
            } else if event_str == "離站" {
                EventType::Depart
            } else {
                continue;
            };

            raw_records.push(RawGps {
                plate_id,
                stop_name_tw,
                event,
                time,
            });
        }
    }

    raw_records.sort_unstable_by(|a, b| {
        a.plate_id.cmp(&b.plate_id).then_with(|| a.time.cmp(&b.time))
    });

    let mut stats = BusStopStats::default();
    stats.raw_records = raw_records.len();

    let mut bus_stops = Vec::with_capacity(raw_records.len() / 2);
    let mut iter = raw_records.into_iter().peekable();

    let mut buffered_stop = get_next_raw_stop(&mut iter, &mut stats);
    let mut prev_depart: Option<u64> = None;
    let mut prev_plate: Option<u32> = None;

    while let Some(mut curr) = buffered_stop {
        let mut next_stop = get_next_raw_stop(&mut iter, &mut stats);
        
        let is_first = prev_plate != Some(curr.plate_id);
        let is_last = next_stop.as_ref().map(|s| s.plate_id) != Some(curr.plate_id);

        // 1. Resolve missing arrival_time (Rule 3 and endpoints)
        if curr.arrival_time.is_none() {
            let c_dep = curr.depart_time.unwrap();
            if is_first {
                curr.arrival_time = Some(c_dep.saturating_sub(600));
            } else {
                let p_dep = prev_depart.unwrap();
                let avg = (p_dep + c_dep) / 2;
                curr.arrival_time = Some(c_dep.saturating_sub(600).max(avg));
            }
        }

        // 2. Resolve missing depart_time (Rule 1, Rule 2, and endpoints)
        if curr.depart_time.is_none() {
            let c_arr = curr.arrival_time.unwrap();
            if is_last {
                curr.depart_time = Some(c_arr + 600);
            } else {
                let mut nxt = next_stop.unwrap(); 
                
                if nxt.arrival_time.is_none() {
                    // Rule 1: Current missing depart, Next missing arrival (Terminal Crossover)
                    let n_dep = nxt.depart_time.unwrap();
                    let t = n_dep.saturating_sub(c_arr);
                    if t > 1800 {
                        // 1a: Wait > 30 mins
                        curr.depart_time = Some(c_arr + 600);
                        nxt.arrival_time = Some(n_dep.saturating_sub(600));
                    } else {
                        // 1b: Wait <= 30 mins
                        let mid = (c_arr + n_dep) / 2;
                        let proposed_depart = mid + 90;
                        curr.depart_time = Some(proposed_depart.min(n_dep.saturating_sub(1)));
                        nxt.arrival_time = Some(curr.depart_time.unwrap() + 1);
                    }
                } else {
                    // Rule 2: Current missing depart, Next HAS arrival
                    let n_arr = nxt.arrival_time.unwrap();
                    let avg = (c_arr + n_arr) / 2;
                    curr.depart_time = Some((c_arr + 600).min(avg));
                }
                next_stop = Some(nxt);
            }
        }

        bus_stops.push(BusStop {
            plate_id: curr.plate_id,
            stop_name_tw: curr.stop_name_tw,
            arrival_time: curr.arrival_time.unwrap(),
            depart_time: curr.depart_time.unwrap(),
            pairing: curr.pairing,
        });

        prev_depart = curr.depart_time;
        prev_plate = Some(curr.plate_id);
        
        buffered_stop = next_stop;
    }

    stats.final_records = bus_stops.len();
    Ok((bus_stops, stats))
}

pub fn build_busstop_plate_index(stops: &[BusStop], plate_count: usize) -> Vec<PlateIndex> {
    let mut indices = vec![PlateIndex::default(); plate_count];
    for (i, s) in stops.iter().enumerate() {
        let idx = &mut indices[s.plate_id as usize];
        if idx.count == 0 {
            idx.start = i;
        }
        idx.count += 1;
    }
    indices
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;
    use crate::utils::date_time_to_timestamp;

    #[test]
    fn test_busstop_pairing_heuristics() -> Result<(), Box<dyn Error>> {
        let mut file = NamedTempFile::new()?;
        writeln!(file, "PlateNumb,OperatorID,OperatorNo,RouteUID,RouteID,RouteNameZh_tw,RouteNameEn,SubRouteUID,SubRouteID,SubRouteNameZh_tw,SubRouteNameEn,Direction,StopUID,StopID,StopNameZh_tw,StopNameEn,StopSequence,MessageType,DutyStatus,BusStatus,A2EventType,GPSTime,TripStartTimeType,TripStartTime,TransTime,SrcRecTime,SrcTransTime,SrcUpdateTime,UpdateTime")?;
        
        writeln!(file, r#""PLATE-A","1","1","T","1","2","2","T0","10","2","2",去程,"S1","1","Stop1","S1",1,,開始,正常,離站,2025-02-11T05:00:00+08:00,實際,"2025-02-11T05:00:00+08:00",,,,2025-02-11T05:00:00+08:00,2025-02-11T05:00:00+08:00"#)?;
        writeln!(file, r#""PLATE-B","1","1","T","1","2","2","T0","10","2","2",去程,"S2","2","Stop2","S2",1,,開始,正常,進站,2025-02-11T10:00:00+08:00,實際,"2025-02-11T10:00:00+08:00",,,,2025-02-11T10:00:00+08:00,2025-02-11T10:00:00+08:00"#)?;

        let mut pm = PlateManager::new(10);
        let mut dict = DictEncoder::new(10, 200);
        let path = file.path().to_str().unwrap().to_string();
        let target_day = logical_day_index(date_time_to_timestamp(b"2025-02-11", b"12:00:00"));

        let (stops, _) = busstop_loader(&[path], target_day, &mut pm, &mut dict)?;

        let plate_a = pm.insert(b"PLATEA");
        let stop_a = stops.iter().find(|s| s.plate_id == plate_a).unwrap();
        assert_eq!(stop_a.pairing, PairingStatus::NoArrival);
        assert_eq!(stop_a.depart_time - stop_a.arrival_time, 600);

        let plate_b = pm.insert(b"PLATEB");
        let stop_b = stops.iter().find(|s| s.plate_id == plate_b).unwrap();
        assert_eq!(stop_b.pairing, PairingStatus::NoDeparture);
        assert_eq!(stop_b.depart_time - stop_b.arrival_time, 600);

        Ok(())
    }
}
