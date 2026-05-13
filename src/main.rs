mod plate_manager;
mod dict_encoder;
mod utils;
mod models;
mod easycard_loader;
mod sorted_index;
mod busstop_loader;
mod matching;

use std::env;
use std::fs;
use log::{info, warn, error};
use plate_manager::PlateManager;
use dict_encoder::DictEncoder;
use easycard_loader::{easycard_loader};
use busstop_loader::{busstop_loader};
use matching::match_and_write_day_results;
use utils::{format_day_index};
use csv::Writer;

fn main() {
    // Set default log level to 'info' if RUST_LOG is not set.
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    
    let program_start_time = std::time::Instant::now(); // Overall program timer

    let mut args: Vec<String> = env::args().collect();
    let mut data_dir = ".".to_string();
    let mut output_route_file = false;
    let mut target_date: Option<String> = None;
    let mut target_plate: Option<String> = None;
    let mut grace_period: u64 = 600; // Default 10 minutes

    // --- Argument Parsing ---
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-d" | "--datadir" => {
                if i + 1 < args.len() {
                    data_dir = args.remove(i + 1);
                    args.remove(i);
                } else {
                    error!("{} flag requires a path.", args[i]);
                    return;
                }
            },
            "-t" | "--grace" => {
                if i + 1 < args.len() {
                    grace_period = args.remove(i + 1).parse().unwrap_or(600);
                    args.remove(i);
                } else {
                    error!("--grace flag requires a value in seconds.");
                    return;
                }
            },
            "-route" => {
                output_route_file = true;
                args.remove(i);
            },
            "--date" => {
                if i + 1 < args.len() {
                    target_date = Some(args.remove(i + 1));
                    args.remove(i);
                } else {
                    error!("--date flag requires a YYYY-MM-DD string.");
                    return;
                }
            },
            "--plate" => {
                if i + 1 < args.len() {
                    target_plate = Some(args.remove(i + 1));
                    args.remove(i);
                } else {
                    error!("--plate flag requires a plate ID.");
                    return;
                }
            },
            _ => i += 1,
        }
    }

    // --- Specialized Mode: Print Bus Stop Pairs ---
    if target_date.is_some() || target_plate.is_some() {
        if target_date.is_none() || target_plate.is_none() {
            error!("Bus Stop Inspect mode requires BOTH --date and --plate to be specified.");
            return;
        }

        let date_str = target_date.unwrap();
        let plate_str = target_plate.unwrap();
        
        use utils::{date_time_to_timestamp, logical_day_index, timestamp_to_time_string};

        info!("Mode: Bus Stop Inspection for Plate {} on {} (grace period {}s)", plate_str, date_str, grace_period);
        
        let ts = date_time_to_timestamp(date_str.as_bytes(), b"12:00:00");
        let logical_day = logical_day_index(ts);

        let mut pm = PlateManager::new(10000);
        let mut stop_name_dict = DictEncoder::new(10000, 200_000);

        let date_d_str = date_str;
        let date_d_plus_1_str = format_day_index(logical_day + 1);
        
        let mut busstop_files = Vec::new();
        if let Ok(entries) = fs::read_dir(&data_dir) {
            for entry in entries.flatten() {
                if let Ok(file_name) = entry.file_name().into_string() {
                    let path_str = entry.path().to_string_lossy().to_string();
                    let is_csv = file_name.ends_with(".CSV") || file_name.ends_with(".csv");
                    if is_csv && (file_name.contains(&date_d_str) || file_name.contains(&date_d_plus_1_str)) {
                        busstop_files.push(path_str);
                    }
                }
            }
        }

        if busstop_files.is_empty() {
            error!("No bus stop files found for {} in {}", date_d_str, data_dir);
            return;
        }

        info!("Loading {} bus stop files...", busstop_files.len());
        let (bus_stops, _stats) = match busstop_loader(&busstop_files, logical_day, &mut pm, &mut stop_name_dict) {
            Ok(res) => res,
            Err(e) => {
                error!("Error loading BusStop data: {}", e);
                return;
            }
        };

        let plate_id = pm.get_id(plate_str.replace("-", "").as_bytes());
        
        let filtered_stops: Vec<_> = if plate_id == u32::MAX {
            Vec::new() // Plate not found in the loaded data at all
        } else {
            bus_stops.iter().filter(|s| s.plate_id == plate_id).collect()
        };

        if filtered_stops.is_empty() {
            warn!("No records found for plate {} on {}", plate_str, date_d_str);
        } else {
            println!("\nBus Stop Pairs for {} on {}:", plate_str, date_d_str);
            println!("{:<30} | {:<8} | {:<8} | Status", "Stop Name", "Arrival", "Depart");
            println!("{}", "-".repeat(65));
            for stop in filtered_stops {
                let stop_name = stop_name_dict.get_string(stop.stop_name_tw);
                println!("{:<30} | {:<8} | {:<8} | {:?}", 
                    stop_name, 
                    timestamp_to_time_string(stop.arrival_time),
                    timestamp_to_time_string(stop.depart_time),
                    stop.pairing
                );
            }
        }
        return;
    }

    let file_paths_args = args.drain(1..).collect::<Vec<String>>();
    if file_paths_args.is_empty() {
        info!("Usage:");
        info!("  Standard mode:  ecbus [-route] [-t <seconds>] [-d /path/to/data] <easycard_csv_file1> ...");
        info!("  Inspect mode:   ecbus --date YYYY-MM-DD --plate PLATE_ID [-d /path/to/data]");
        return;
    }

    info!("Scanning for bus stop data in: {}", data_dir);

    let file_paths: Vec<&str> = file_paths_args.iter().map(|s| s.as_str()).collect();
    
    let mut pm = PlateManager::new(10000);
    let mut company_dict = DictEncoder::new(100, 2000);
    let mut tx_type_dict = DictEncoder::new(100, 2000);

    let (transactions, day_indices) = match easycard_loader(&file_paths, &mut pm, &mut tx_type_dict, &mut company_dict) {
        Ok(res) => res,
        Err(e) => {
            error!("Error loading EasyCard: {}", e);
            return;
        }
    };

    info!("Total EasyCard records loaded: {}", transactions.len());
    
    let mut stop_name_dict = DictEncoder::new(10000, 200_000);

    for day in day_indices {
        let day_processing_start_time = std::time::Instant::now();

        let date_d_str = format_day_index(day.logical_day);
        let tx_day_slice = &transactions[day.start .. day.start + day.count];
        let date_d_plus_1_str = format_day_index(day.logical_day + 1);
        
        let mut busstop_files = Vec::new();
        if let Ok(entries) = fs::read_dir(&data_dir) {
            for entry in entries.flatten() {
                if let Ok(file_name) = entry.file_name().into_string() {
                    let path_str = entry.path().to_string_lossy().to_string();
                    
                    if file_paths.contains(&path_str.as_str()) { continue; }

                    let is_csv = file_name.ends_with(".CSV") || file_name.ends_with(".csv");
                    if is_csv && (file_name.contains(&date_d_str) || file_name.contains(&date_d_plus_1_str)) {
                        busstop_files.push(path_str);
                    }
                }
            }
        }

        if busstop_files.is_empty() {
            warn!("Missing bus stop file for {}", date_d_str);
            let day_processing_duration = day_processing_start_time.elapsed();
            info!("Completed {} in {} ms", date_d_str, day_processing_duration.as_millis());
            continue;
        }
        
        let (bus_stops, _stats) = match busstop_loader(&busstop_files, day.logical_day, &mut pm, &mut stop_name_dict) {
            Ok(res) => res,
            Err(e) => {
                error!("Error loading BusStop data for {}: {}", date_d_str, e);
                let day_processing_duration = day_processing_start_time.elapsed();
                info!("Completed {} in {} ms", date_d_str, day_processing_duration.as_millis());
                continue;
            }
        };

        if bus_stops.is_empty() {
            warn!("No bus stop data for {} after filtering.", date_d_str);
            let day_processing_duration = day_processing_start_time.elapsed();
            info!("Completed {} in {} ms", date_d_str, day_processing_duration.as_millis());
            continue;
        }

        info!("Processing {}: {} txs vs {} stops", 
            date_d_str, tx_day_slice.len(), bus_stops.len());

        let output_filename = format!("ecbus_{}.csv", date_d_str);
        
        let mut route_writer_opt: Option<Writer<std::fs::File>> = None;
        if output_route_file {
            let route_filename = format!("route_{}.csv", date_d_str);
            info!("Outputting to {} & {}", output_filename, route_filename);
            let mut writer = Writer::from_path(&route_filename).expect("Failed to create route file");
            writer.write_record(&[
                "card_id", "tx_on_time", "tx_off_time", 
                "stop_name", "stop_arrival_time", "stop_depart_time", "pairing_status"
            ]).expect("Failed to write route header");
            route_writer_opt = Some(writer);
        } else {
            info!("Outputting to {}", output_filename);
        }
        
        match_and_write_day_results(
            &output_filename,
            tx_day_slice,
            &bus_stops,
            &pm,
            &tx_type_dict,
            &company_dict,
            &stop_name_dict,
            &mut route_writer_opt,
            output_route_file,
            grace_period,
        ).unwrap();

        let day_processing_duration = day_processing_start_time.elapsed();
        info!("Completed {} in {} ms", date_d_str, day_processing_duration.as_millis());
    }

    let total_program_duration = program_start_time.elapsed();
    info!("Total program running time: {:?}", total_program_duration);
}

