use crate::models::Transaction;

/// A node in the sorted index linking back to the original vector.
/// It will later be modified to link to another vector as well.
#[derive(Debug, Clone)]
pub struct SortedNode {
    pub original_index: u32,
    // Add additional link fields here in the future
}

/// Manages the sorted index array and handles bi-directional linking.
pub struct TransactionIndex {
    pub nodes: Vec<SortedNode>,
}

impl TransactionIndex {
    /// Builds the sorted index from a mutable slice of transactions.
    /// It sorts indirectly based on `plate_id` and then `on_time`.
    /// It also mutates the original transactions to set their `sorted_index`.
    pub fn build(transactions: &mut [Transaction]) -> Self {
        // Create an array of nodes pointing to the original indices
        let mut nodes: Vec<SortedNode> = (0..transactions.len() as u32)
            .map(|i| SortedNode { original_index: i })
            .collect();

        // Sort nodes based on the values in the original vector
        // Primary: plate_id, Secondary: on_time
        nodes.sort_unstable_by(|a, b| {
            let tx_a = &transactions[a.original_index as usize];
            let tx_b = &transactions[b.original_index as usize];
            
            tx_a.plate_id.cmp(&tx_b.plate_id)
                .then_with(|| tx_a.on_time.cmp(&tx_b.on_time))
        });

        // Bi-directional linkage: update the original vector with their new sorted positions
        for (sorted_idx, node) in nodes.iter().enumerate() {
            transactions[node.original_index as usize].sorted_index = sorted_idx as u32;
        }

        Self { nodes }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bi_directional_sort() {
        // Base timestamp: 2026-05-06 12:00:00 UTC (1778068800)
        // Logical day index (1778068800 + 18000) / 86400 = 20579
        let base_ts = 1778068800;
        let day_idx = 20579;
        
        let mut transactions = vec![
            Transaction { card_id: 1, plate_id: 2, on_time: base_ts + 100, off_time: base_ts + 110, logical_day: day_idx, sorted_index: 0, seq_no_on: 0, seq_no_off: 0, on_off_code: [0; 8], tx_type_id: 0, company_id: 0, line_no: 0 },
            Transaction { card_id: 2, plate_id: 1, on_time: base_ts + 200, off_time: base_ts + 210, logical_day: day_idx, sorted_index: 0, seq_no_on: 0, seq_no_off: 0, on_off_code: [0; 8], tx_type_id: 0, company_id: 0, line_no: 0 },
            Transaction { card_id: 3, plate_id: 2, on_time: base_ts + 50,  off_time: base_ts + 60,  logical_day: day_idx, sorted_index: 0, seq_no_on: 0, seq_no_off: 0, on_off_code: [0; 8], tx_type_id: 0, company_id: 0, line_no: 0 },
            Transaction { card_id: 4, plate_id: 1, on_time: base_ts + 150, off_time: base_ts + 160, logical_day: day_idx, sorted_index: 0, seq_no_on: 0, seq_no_off: 0, on_off_code: [0; 8], tx_type_id: 0, company_id: 0, line_no: 0 },
        ];

        let index = TransactionIndex::build(&mut transactions);

        // Expected sorted order:
        // 1. plate 1, time 150 (orig idx 3)
        // 2. plate 1, time 200 (orig idx 1)
        // 3. plate 2, time 50  (orig idx 2)
        // 4. plate 2, time 100 (orig idx 0)

        assert_eq!(index.nodes.len(), 4);

        // Verify sorted nodes points to correct original index
        assert_eq!(index.nodes[0].original_index, 3);
        assert_eq!(index.nodes[1].original_index, 1);
        assert_eq!(index.nodes[2].original_index, 2);
        assert_eq!(index.nodes[3].original_index, 0);

        // Verify bi-directional linkage: original -> sorted_index
        assert_eq!(transactions[3].sorted_index, 0);
        assert_eq!(transactions[1].sorted_index, 1);
        assert_eq!(transactions[2].sorted_index, 2);
        assert_eq!(transactions[0].sorted_index, 3);
    }
}
