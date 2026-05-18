use std::error::Error;
use csv::Writer;
use crate::models::{Transaction, BusStop, PairingStatus};
use crate::plate_manager::PlateManager;
use crate::dict_encoder::DictEncoder;
use crate::utils::{timestamp_to_date_string, timestamp_to_time_string, format_on_off_code};
use crate::busstop_loader::build_busstop_plate_index;

pub fn match_and_write_day_results(
    output_filename: &str,
    tx_slice: &[Transaction],
    bus_stops: &[BusStop],
    pm: &PlateManager,
    tx_type_dict: &DictEncoder,
    company_dict: &DictEncoder,
    stop_name_dict: &DictEncoder,
    route_writer_opt: &mut Option<Writer<std::fs::File>>,
    output_route_file: bool,
    grace_period: u64,
) -> Result<(), Box<dyn Error>> {
    let mut writer = Writer::from_path(output_filename)?;

    writer.write_record(&[
        "卡號", "NEW__SEQ_NO_上下車資料源", "交易種類", "上下車代號", 
        "sp_shortname", "上車日期", "上車時間", "下車日期", "下車時間", 
        "LINE_NO", "BUS_NO", "序號連續檢索參考", "OD檢索參考", "時間檢索參考", 
        "RouteNameZh_tw", "board_stop", "alight_stop", "route", 
        "mapping_status", "配對參考"
    ])?;

    let stop_plate_index = build_busstop_plate_index(bus_stops, pm.len());

    for tx in tx_slice {
        let stop_idx = &stop_plate_index[tx.plate_id as usize];
        let current_stop_slice = if stop_idx.count > 0 {
            &bus_stops[stop_idx.start .. stop_idx.start + stop_idx.count]
        } else {
            &[]
        };

        let mut onboard_stop_idx: Option<usize> = None;
        let mut alight_stop_idx: Option<usize> = None;

        if !current_stop_slice.is_empty() {
            let mut p1 = 0;
            while p1 < current_stop_slice.len() {
                let next_arrival = if p1 + 1 < current_stop_slice.len() {
                    current_stop_slice[p1 + 1].arrival_time
                } else {
                    u64::MAX
                };

                if tx.on_time < next_arrival {
                    onboard_stop_idx = Some(p1);
                    break;
                }
                p1 += 1;
            }

            // Fallback for the very last stop if on_time is after it
            if onboard_stop_idx.is_none() && !current_stop_slice.is_empty() {
                if tx.on_time >= current_stop_slice.last().unwrap().arrival_time {
                    onboard_stop_idx = Some(current_stop_slice.len() - 1);
                }
            }

            if tx.off_time > 0 && onboard_stop_idx.is_some() {
                let mut p2 = onboard_stop_idx.unwrap();
                while p2 < current_stop_slice.len() {
                    if current_stop_slice[p2].depart_time >= tx.off_time {
                        alight_stop_idx = Some(p2);
                        break;
                    }
                    p2 += 1;
                }
            }

            // Apply Dual-Distance Grace Period Rule
            if let Some(idx) = onboard_stop_idx {
                let stop = &current_stop_slice[idx];
                let diff_arr = tx.on_time.abs_diff(stop.arrival_time);
                let diff_dep = tx.on_time.abs_diff(stop.depart_time);
                if diff_arr > grace_period && diff_dep > grace_period {
                    onboard_stop_idx = None;
                }
            }
            if let Some(idx) = alight_stop_idx {
                if tx.off_time > 0 {
                    let stop = &current_stop_slice[idx];
                    let diff_arr = tx.off_time.abs_diff(stop.arrival_time);
                    let diff_dep = tx.off_time.abs_diff(stop.depart_time);
                    if diff_arr > grace_period && diff_dep > grace_period {
                        alight_stop_idx = None;
                    }
                }
            }
        }
        
        let on_date = timestamp_to_date_string(tx.on_time);
        let on_time_str = timestamp_to_time_string(tx.on_time);
        
        let (off_date, off_time_str) = if tx.off_time > 0 {
            (timestamp_to_date_string(tx.off_time), timestamp_to_time_string(tx.off_time))
        } else {
            ("".to_string(), "".to_string())
        };
        
        let (status1, status2, status3) = if tx.off_time > 0 {
            ("正常", "OD_OK", "時間_OK")
        } else {
            ("異常", "OD_NG", "時間_NG")
        };

        // Decide overall pair status
        let is_perfect_pair = onboard_stop_idx.is_some() && (tx.off_time == 0 || alight_stop_idx.is_some());

        let board_stop = if let Some(idx) = onboard_stop_idx {
            stop_name_dict.get_string(current_stop_slice[idx].stop_name_tw).to_string()
        } else {
            "NULL".to_string()
        };

        let alight_stop = if let Some(idx) = alight_stop_idx {
            stop_name_dict.get_string(current_stop_slice[idx].stop_name_tw).to_string()
        } else {
            "NULL".to_string()
        };

        let route = if let (Some(s_idx), Some(t_idx)) = (onboard_stop_idx, alight_stop_idx) {
            if t_idx >= s_idx {
                let mut route_parts = Vec::new();
                let mut i = s_idx;
                while i <= t_idx {
                    let start_stop = &current_stop_slice[i];
                    let stop_name = stop_name_dict.get_string(start_stop.stop_name_tw);
                    let arrival_time = start_stop.arrival_time;
                    let mut depart_time = start_stop.depart_time;

                    // Look ahead for consecutive identical stops
                    let mut j = i + 1;
                    while j <= t_idx {
                        let next_stop = &current_stop_slice[j];
                        if stop_name_dict.get_string(next_stop.stop_name_tw) == stop_name {
                            depart_time = next_stop.depart_time; // Update to the last depart time
                            j += 1;
                        } else {
                            break;
                        }
                    }

                    let time_str = if i == t_idx {
                        timestamp_to_time_string(depart_time)
                    } else {
                        timestamp_to_time_string(arrival_time)
                    };
                    route_parts.push(format!("{}({})", stop_name, time_str));
                    
                    i = j; // Move index past all processed duplicates
                }
                route_parts.join(",")
            } else {
                "NULL".to_string()
            }
        } else {
            "NULL".to_string()
        };
        
        let (map_status, pair_ref) = if is_perfect_pair {
            ("配對_OK", "OK")
        } else if onboard_stop_idx.is_some() || alight_stop_idx.is_some() {
            ("配對_部分", "Partial")
        } else {
            ("配對_NG", "NG")
        };

        if output_route_file {
            if let Some(writer) = route_writer_opt.as_mut() {
                if let (Some(s_idx), Some(t_idx)) = (onboard_stop_idx, alight_stop_idx) {
                    // Full route found
                    let mut i = s_idx;
                    while i <= t_idx {
                        let start_stop = &current_stop_slice[i];
                        let stop_name = stop_name_dict.get_string(start_stop.stop_name_tw).to_string();
                        let arrival_time = start_stop.arrival_time;
                        let mut depart_time = start_stop.depart_time;

                        // Look ahead for consecutive identical stops
                        let mut j = i + 1;
                        while j <= t_idx {
                            let next_stop = &current_stop_slice[j];
                            if stop_name_dict.get_string(next_stop.stop_name_tw) == stop_name {
                                depart_time = next_stop.depart_time; // Update to the last depart time
                                j += 1;
                            } else {
                                break;
                            }
                        }
                        
                        let status_str = match start_stop.pairing {
                            PairingStatus::Ok => "OK",
                            PairingStatus::NoArrival => "NoArr",
                            PairingStatus::NoDeparture => "NoDptr",
                        };

                        writer.write_record(&[
                            tx.card_id.to_string(),
                            timestamp_to_time_string(tx.on_time).to_string(),
                            timestamp_to_time_string(tx.off_time).to_string(),
                            stop_name,
                            timestamp_to_time_string(arrival_time).to_string(),
                            timestamp_to_time_string(depart_time).to_string(),
                            status_str.to_string(),
                        ])?;
                        
                        i = j; // Move index past all processed duplicates
                    }
                } else {
                    // Partial or no route found. Report on_stop and off_stop separately.
                    
                    // 1. Report Boarding Stop (or NULL)
                    if let Some(s_idx) = onboard_stop_idx {
                        let stop = &current_stop_slice[s_idx];
                        writer.write_record(&[
                            tx.card_id.to_string(),
                            timestamp_to_time_string(tx.on_time).to_string(),
                            if tx.off_time > 0 { timestamp_to_time_string(tx.off_time).to_string() } else { "".to_string() },
                            stop_name_dict.get_string(stop.stop_name_tw).to_string(),
                            timestamp_to_time_string(stop.arrival_time).to_string(),
                            timestamp_to_time_string(stop.depart_time).to_string(),
                            "OK".to_string(),
                        ])?;
                    } else {
                        writer.write_record(&[
                            tx.card_id.to_string(),
                            timestamp_to_time_string(tx.on_time).to_string(),
                            if tx.off_time > 0 { timestamp_to_time_string(tx.off_time).to_string() } else { "".to_string() },
                            "NULL".to_string(),
                            "NULL".to_string(),
                            "NULL".to_string(),
                            "NULL".to_string(),
                        ])?;
                    }

                    // 2. Report Alighting Stop (or NULL) if off_time exists
                    if tx.off_time > 0 {
                        if let Some(t_idx) = alight_stop_idx {
                            let stop = &current_stop_slice[t_idx];
                            writer.write_record(&[
                                tx.card_id.to_string(),
                                timestamp_to_time_string(tx.on_time).to_string(),
                                timestamp_to_time_string(tx.off_time).to_string(),
                                stop_name_dict.get_string(stop.stop_name_tw).to_string(),
                                timestamp_to_time_string(stop.arrival_time).to_string(),
                                timestamp_to_time_string(stop.depart_time).to_string(),
                                "OK".to_string(),
                            ])?;
                        } else {
                            writer.write_record(&[
                                tx.card_id.to_string(),
                                timestamp_to_time_string(tx.on_time).to_string(),
                                timestamp_to_time_string(tx.off_time).to_string(),
                                "NULL".to_string(),
                                "NULL".to_string(),
                                "NULL".to_string(),
                                "NULL".to_string(),
                            ])?;
                        }
                    }
                }
            }
        }

        writer.write_record(&[
            tx.card_id.to_string(),
            format!("{},{}", tx.seq_no_on, tx.seq_no_off),
            tx_type_dict.get_string(tx.tx_type_id).to_string(),
            format_on_off_code(&tx.on_off_code).to_string(),
            company_dict.get_string(tx.company_id).to_string(),
            on_date.to_string(),
            on_time_str.to_string(),
            off_date.to_string(),
            off_time_str.to_string(),
            tx.line_no.to_string(),
            pm.get_string(tx.plate_id).to_string(),
            status1.to_string(),
            status2.to_string(),
            status3.to_string(),
            "".to_string(), // RouteNameZh_tw
            board_stop,
            alight_stop,
            route,
            map_status.to_string(),
            pair_ref.to_string(),
        ])?;
    }

    if let Some(writer) = route_writer_opt {
        writer.flush()?;
    }
    Ok(())
}
