use hex;
use primitive_types::U256;
use sha2::{Digest, Sha256};
use super::vdf_solution::{HCGraphUtil, GRAPH_SIZE};

fn parse_header_time_from_data(header_data_hex: &str) -> u32 {
    let time_start = 8 + 64 + 64;
    let time_le = &header_data_hex[time_start..time_start + 8];
    let mut bytes = [0u8; 4];
    for i in 0..4 {
        bytes[i] = u8::from_str_radix(&time_le[i * 2..i * 2 + 2], 16).unwrap();
    }
    u32::from_be_bytes([bytes[3], bytes[2], bytes[1], bytes[0]])
}

const HARDFORK_TIMESTAMP_QB: u32 = 1759204800;

pub fn compute_hash_no_vdf(data: &str, hc_util: &mut HCGraphUtil) -> Option<(String, String)> {
    // Create the vdfSolution array with all values set to 0xFFFF (uint16_t max value)
    let vdf_solution: Vec<u16> = vec![0xFFFF; GRAPH_SIZE.into()];

    // Convert vdfSolution to a hex string
    let vdf_solution_hex: String = vdf_solution
        .iter()
        .map(|&val| format!("{:04x}", val))
        .collect();

    // Append vdfSolution hex to the input data
    let data_with_vdf = format!("{}{}", data, vdf_solution_hex);

    // Convert the hex string to bytes
    let data_bytes = hex::decode(data_with_vdf).expect("Invalid hex input");

    // First SHA256 hash
    let mut hasher = Sha256::new();
    hasher.update(&data_bytes);
    let hash1 = hasher.finalize();
    
    let hash1_reversed = hex::encode(hash1.iter().rev().cloned().collect::<Vec<u8>>());
    let graph_hash_u256 = U256::from_str_radix(&hash1_reversed, 16).unwrap();

    // Get worker and queen bee grid sizes
    let hash1_hex = format!("{:064x}", graph_hash_u256);
    let worker_grid_size = hc_util.get_worker_grid_size(&hash1_hex);
    let queen_bee_grid_size = hc_util.get_queen_bee_grid_size(worker_grid_size);

    let header_time = parse_header_time_from_data(data);
    
    let mut combined_path: Vec<u16>;
    
    if header_time <= HARDFORK_TIMESTAMP_QB {
        let path_v2 = hc_util.find_hamiltonian_cycle_v2(graph_hash_u256);
        if path_v2.is_empty() {
            return None;
        }
        combined_path = path_v2;
    } else {
        let worker_path = hc_util.find_hamiltonian_cycle_v3_hex(&hash1_hex, worker_grid_size, 500, 1000);
        
        if worker_path.is_empty() {
            return None;
        }

        // Bitcoin Core HashWriter serialization: << worker_solution << first_hash
        let mut queen_hash_data = Vec::new();
        
        // Serialize vector size as compact integer (Bitcoin Core style)
        let size = worker_path.len();
        if size < 0xfd {
            queen_hash_data.push(size as u8);
        } else if size <= 0xffff {
            queen_hash_data.push(0xfd);
            queen_hash_data.extend(&(size as u16).to_le_bytes());
        } else {
            queen_hash_data.push(0xfe);
            queen_hash_data.extend(&(size as u32).to_le_bytes());
        }
        
        // Serialize each uint16_t in little-endian format
        for &val in &worker_path {
            queen_hash_data.extend(&val.to_le_bytes());
        }
        
        // Append first_hash bytes (32 bytes)
        let mut hash1_bytes = hex::decode(&hash1_hex).expect("Invalid hex");
        hash1_bytes.reverse();
        queen_hash_data.extend(&hash1_bytes);
        
        let mut queen_hasher = Sha256::new();
        queen_hasher.update(&queen_hash_data);
        let queen_hash = queen_hasher.finalize();
        let queen_hash_reversed = hex::encode(queen_hash.iter().rev().cloned().collect::<Vec<u8>>());

        let queen_path = hc_util.find_hamiltonian_cycle_v3_hex(&queen_hash_reversed, queen_bee_grid_size, 125, 10);
        if queen_path.is_empty() {
            return None;
        }

        combined_path = worker_path.clone();
        combined_path.extend(queen_path.clone());
    }

    // Ensure combined path matches expected size
    if combined_path.len() < GRAPH_SIZE.into() {
        combined_path.resize(GRAPH_SIZE.into(), u16::MAX);
    }

    // Format combined path as little-endian hex string
    let vdf_solution_hex_solved: String = combined_path
        .iter()
        .map(|&val| {
            let little_endian_val = val.to_le_bytes();
            format!("{:02x}{:02x}", little_endian_val[0], little_endian_val[1])
        })
        .collect();
    
    let data_with_vdf_solved = format!("{}{}", data, vdf_solution_hex_solved);

    let data_bytes_solved = hex::decode(data_with_vdf_solved).expect("Invalid hex input");

    // Final SHA256 hash
    let mut hasher2 = Sha256::new();
    hasher2.update(&data_bytes_solved);
    let hash2 = hasher2.finalize();

    let final_hash_reversed = hex::encode(hash2.iter().rev().cloned().collect::<Vec<u8>>());

    Some((final_hash_reversed, vdf_solution_hex_solved))
}
