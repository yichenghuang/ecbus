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
    let mut stop_scan_indices = vec![0; pm.len()]; 

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
            let mut p1 = stop_scan_indices[tx.plate_id as usize];
            while p1 + 1 < current_stop_slice.len() {
                if current_stop_slice[p1].arrival_time <= tx.on_time &&
                   current_stop_slice[p1 + 1].arrival_time > tx.on_time {
                    onboard_stop_idx = Some(p1);
                    stop_scan_indices[tx.plate_id as usize] = p1; 
                    break;
                }
                p1 += 1;
            }

            // Fallback for the last stop
            if onboard_stop_idx.is_none() && current_stop_slice.last().unwrap().arrival_time <= tx.on_time {
                onboard_stop_idx = Some(current_stop_slice.len() - 1);
                stop_scan_indices[tx.plate_id as usize] = current_stop_slice.len() - 1;
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

            // Apply Grace Period Rule
            if let Some(idx) = onboard_stop_idx {
                if tx.on_time.saturating_sub(current_stop_slice[idx].arrival_time) > grace_period {
                    onboard_stop_idx = None;
                }
            }
            if let Some(idx) = alight_stop_idx {
                if tx.off_time > 0 && current_stop_slice[idx].depart_time.saturating_sub(tx.off_time) > grace_period {
                    alight_stop_idx = None;
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

        // If off_time is > 0, we MUST have found an alight_stop_idx.
        // If off_time == 0, we don't have an alight stop, but we might still consider it valid if onboard was found.
        // The user requested: "If either on_time, or off_time is unfound or cannot be found... output NULL".
        let is_valid_match = onboard_stop_idx.is_some() && (tx.off_time == 0 || alight_stop_idx.is_some());

        let (board_stop, alight_stop, route) = if is_valid_match {
            let b = stop_name_dict.get_string(current_stop_slice[onboard_stop_idx.unwrap()].stop_name_tw).to_string();
            let a = if let Some(idx) = alight_stop_idx {
                stop_name_dict.get_string(current_stop_slice[idx].stop_name_tw).to_string()
            } else {
                "".to_string()
            };
            
            let r = if let (Some(s_idx), Some(t_idx)) = (onboard_stop_idx, alight_stop_idx) {
                if t_idx >= s_idx {
                    current_stop_slice[s_idx..=t_idx].iter()
                        .map(|s| stop_name_dict.get_string(s.stop_name_tw))
                        .collect::<Vec<_>>()
                        .join(",")
                } else {
                    "".to_string()
                }
            } else {
                "".to_string()
            };
            (b, a, r)
        } else {
            ("NULL".to_string(), "NULL".to_string(), "NULL".to_string())
        };
        
        let (map_status, pair_ref) = if is_valid_match {
            ("配對_OK", "OK")
        } else {
            ("配對_NG", "NG")
        };

        if output_route_file {
            if let Some(writer) = route_writer_opt.as_mut() {
                if let (Some(s_idx), Some(t_idx)) = (onboard_stop_idx, alight_stop_idx) {
                    // Full route found
                    for i in s_idx..=t_idx {
                        let stop = &current_stop_slice[i];
                        let status_str = match stop.pairing {
                            PairingStatus::Ok => "OK",
                            PairingStatus::NoArrival => "NoArr",
                            PairingStatus::NoDeparture => "NoDptr",
                        };
                        writer.write_record(&[
                            tx.card_id.to_string(),
                            timestamp_to_time_string(tx.on_time).to_string(),
                            timestamp_to_time_string(tx.off_time).to_string(),
                            stop_name_dict.get_string(stop.stop_name_tw).to_string(),
                            timestamp_to_time_string(stop.arrival_time).to_string(),
                            timestamp_to_time_string(stop.depart_time).to_string(),
                            status_str.to_string(),
                        ])?;
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
