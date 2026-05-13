# ECBUS CLI Tool

`ecbus` is a high-performance Rust command-line interface (CLI) tool designed to process and analyze public transportation data. It efficiently loads EasyCard transaction records and bus GPS logs, merges them based on logical day boundaries, and matches individual transactions to the exact bus stops and routes taken.

## Features

-   **High-Performance CSV Parsing**: Utilizes `csv::ByteRecord` for fast, memory-efficient processing of large CSV datasets.
-   **Optimized String Deduplication**: Employs `PlateManager` (for fixed-length plate IDs) and `DictEncoder` (for variable-length stop names, transaction types, company names) to minimize memory footprint and enable `O(1)` string-to-ID lookups.
-   **Logical Day Processing**: Defines a "logical day" starting at 3:00 AM to correctly handle late-night and early-morning public transport services that cross midnight.
-   **Hierarchical Indexing**: Implements `DayIndex` for `O(1)` access to all records for a specific logical day, and on-demand `PlateIndex` for efficient per-plate access within a day.
-   **Advanced BusStop Pairing Heuristics**: Employs a sophisticated single-pass algorithm to pair 'Arrive' and 'Depart' GPS events, synthesizing missing timestamps with non-overlapping heuristics (e.g., +/- 10 mins, 30 seconds, or midpoint averages) while maintaining chronological integrity.
-   **EasyCard Transaction Matching**: Performs a sorted merge-join between EasyCard transactions and corresponding BusStop GPS logs to identify boarding/alighting stops and traversed routes.
-   **Conditional Output**: Generates detailed route verification files only when explicitly requested via a command-line flag.


## How to Run

### Prerequisites
-   Rust (and Cargo) installed.

### Compilation
Navigate to the project root and compile the application in release mode for optimal performance:
```bash
cargo build --release
```
This will create an executable at `target/release/ecbus`.

### Running the Program

The `ecbus` tool supports two operating modes:

1. **EasyCard Processing Mode** — process one or more EasyCard transaction CSV files.
2. **Bus Stop Query Mode** — read bus stop logs and print records filtered by date and/or plate ID.

The `-d` or `--datadir` flag can be used in both modes to specify the directory containing bus stop data files. If not provided, the current working directory (`.`) is used by default.

The `ecbus` tool accepts one or more EasyCard CSV files as input. You can optionally include the `-route` flag to generate detailed route verification files, and the `-d` or `--datadir` flag to specify the directory containing bus stop data. If `-d` is not provided, the current working directory (`.`) will be used by default.

---

## 1. EasyCard Processing Mode

Process one or more EasyCard CSV files as input.

### Basic Usage
```bash
ecbus [-d /path/to/data] <path_to_easycard_file1.csv> [path_to_easycard_file2.csv ...]
```

Example (using default data directory): 
```bash
ecbus ../ecdata/easycard_2025-02-11.csv
```

Example (specifying data directory):
```bash
ecbus -d ../ecdata/ ../ecdata/easycard_2025-02-11.csv
```

### Route File Generation

Include the `-route` flag to generate detailed route verification files.


```bash
ecbus -route [-d /path/to/data] <path_to_easycard_file1.csv> [path_to_easycard_file2.csv ...]
```

Example (specifying data directory):
```bash
ecbus -route -d ../ecdata/ ../ecdata/easycard_2025-02-11.csv
```

### Output Files

For each unique "logical day" found in your EasyCard transaction data, the program will generate two files:

1.  **`ecbus_YYYY-MM-DD.csv`**: The primary output file containing matched EasyCard transactions.
    -   Includes original transaction details, decoded `BUS_NO`, `LINE_NO`, `board_stop`, `alight_stop`, the full `route` (comma-separated stop names), and `mapping_status`.
2.  **`route_YYYY-MM-DD.csv` (Optional, generated with `-route` flag)**: A detailed log for verification purposes.
    -   Lists each stop on a matched route, including the `card_id`, transaction `on_time` and `off_time`, the `stop_name`, `stop_arrival_time`, `stop_depart_time`, and `pairing_status` of the individual bus stop event (e.g., `OK`, `NoArr`, `NoDptr`).
    
## 2. Bus Stop Query Mode

Read bus stop logs and print route for specified plate.

### Usage
```bash
ecbus [-d /path/to/data] --date <YYYY-MM-DD> --plate <PLATE_ID>
```

### Examples

Query records for a specific plate ID:
```bash
ecbus --date 2025-02-11 --plate KKA-1234
```

Using a custom data directory:
```bash
ecbus -d ../ecdata/ --date 2025-02-11 --plate KKA-1234
```


