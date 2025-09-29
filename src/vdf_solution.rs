use primitive_types::U256;
use rand_mt::Mt19937GenRand64;
use crate::graph_bridge::ffi;
use std::time::{Duration, Instant};

pub const GRAPH_SIZE: u16 = 2008;

pub struct HCGraphUtil {
    start_time: Instant,
    vdf_bailout: u64
}

impl HCGraphUtil {
    pub fn new(vdf_bailout: Option<u64>) -> Self {
        let bailout_timer: u64 = match vdf_bailout {
            Some(timer) => { timer },
            None => { 1000 } // default to 1 second
        };
        HCGraphUtil {
            start_time: Instant::now(),
            vdf_bailout: bailout_timer
        }
    }

    fn hex_to_u64(&self, hex_string: &str) -> u64 {
        u64::from_str_radix(hex_string, 16).expect("Failed to convert hex to u64")
    }

    fn extract_seed_from_hash(&self, hash: &U256) -> u64 {
        hash.low_u64()
    }

    fn extract_seed_from_hash_hex(&self, hash_hex: &str) -> u64 {
        let mut bytes = hex::decode(hash_hex).expect("invalid hex");
        bytes.reverse(); // Match C++ implementation that reverses hash bytes
        let arr: [u8; 8] = bytes[0..8].try_into().expect("slice len");
        u64::from_le_bytes(arr)
    }
    

    fn get_grid_size_v2(&self, hash: &U256) -> u16 {
        let hash_hex = format!("{:064x}", hash);
        let grid_size_segment = &hash_hex[0..8];
        let grid_size: u64 = self.hex_to_u64(grid_size_segment);

        let min_grid_size = 2000u64;
        let max_grid_size = GRAPH_SIZE as u64;

        let mut grid_size_final = min_grid_size + (grid_size % (max_grid_size - min_grid_size));
        if grid_size_final > max_grid_size {
            grid_size_final = max_grid_size;
        }
        grid_size_final as u16
    }

    fn generate_graph_v2(&self, hash: &U256, grid_size: u16) -> Vec<Vec<bool>> {
        let grid_size = grid_size as usize;
        let mut graph = vec![vec![false; grid_size]; grid_size];
        let num_edges = (grid_size * (grid_size - 1)) / 2;
        let bits_needed = num_edges;
    
        let seed = self.extract_seed_from_hash(hash);
        let mut prng = Mt19937GenRand64::from(seed.to_le_bytes());
    
        let mut bit_stream = Vec::with_capacity(bits_needed);
    
        while bit_stream.len() < bits_needed {
            let random_bits_32: u32 = (prng.next_u64() & 0xFFFFFFFF) as u32;
            for j in (0..32).rev() {
                if bit_stream.len() >= bits_needed {
                    break;
                }
                let bit = ((random_bits_32 >> j) & 1) == 1;
                bit_stream.push(bit);
            }
        }

        let mut bit_index = 0;
        for i in 0..grid_size {
            for j in (i + 1)..grid_size {
                let edge_exists = bit_stream[bit_index];
                bit_index += 1;
                graph[i][j] = edge_exists;
                graph[j][i] = edge_exists;
            }
        }
    
        graph
    }    

    fn is_safe(&self, v: u16, graph: &Vec<Vec<bool>>, path: &[u16], pos: usize) -> bool {
        if !graph[path[pos - 1] as usize][v as usize] {
            return false;
        }

        for i in 0..pos {
            if path[i] == v {
                return false;
            }
        }

        true
    }

    fn hamiltonian_cycle_util(
        &mut self,
        graph: &Vec<Vec<bool>>,
        path: &mut [u16],
        pos: usize,
    ) -> bool {
        let elapsed = self.start_time.elapsed();
        if elapsed > Duration::from_millis(self.vdf_bailout) {
            return false;
        }

        if pos == graph.len() {
            return graph[path[pos - 1] as usize][path[0] as usize];
        }

        for v in 1..graph.len() as u16 {
            if self.is_safe(v, graph, path, pos) {
                path[pos] = v;

                if self.hamiltonian_cycle_util(graph, path, pos + 1) {
                    return true;
                }

                path[pos] = u16::MAX;
            }
        }

        false
    }

    pub fn find_hamiltonian_cycle_v2(&mut self, graph_hash: U256) -> Vec<u16> {
        let grid_size = self.get_grid_size_v2(&graph_hash);
        let graph = self.generate_graph_v2(&graph_hash, grid_size);

        let mut path = vec![u16::MAX; graph.len()];
        path[0] = 0;
        self.start_time = Instant::now();

        if !self.hamiltonian_cycle_util(&graph, &mut path, 1) {
            return vec![];
        }
        path
    }

    pub fn get_worker_grid_size(&self, hash_hex: &str) -> u16 {
        let grid_size_segment = &hash_hex[0..8];
        let grid_size: u64 = self.hex_to_u64(grid_size_segment);
        let min_grid_size = 1892u64;
        let max_grid_size = 1920u64;
        let grid_size_final = min_grid_size + (grid_size % (max_grid_size - min_grid_size));
        grid_size_final as u16
    }

    pub fn get_queen_bee_grid_size(&self, worker_size: u16) -> u16 {
        GRAPH_SIZE - worker_size
    }


    fn generate_graph_v3_from_seed(&self, seed: u64, grid_size: u16, percentage_x10: u16) -> Vec<Vec<bool>> {
        let grid_size_usize = grid_size as usize;
        let mut graph = vec![vec![false; grid_size_usize]; grid_size_usize];

        let range: u64 = 1000;
        let threshold: u64 = (percentage_x10 as u64 * range) / 1000;

        // Use C++ std::uniform_int_distribution through FFI bridge
        let mut generator = ffi::create_graph_generator(seed, range);
        
        for i in 0..grid_size_usize {
            for j in (i + 1)..grid_size_usize {
                let random_value = generator.pin_mut().next_random();
                let edge_exists = random_value < threshold;
                
                graph[i][j] = edge_exists;
                graph[j][i] = edge_exists;
            }
        }

        graph
    }


    pub fn find_hamiltonian_cycle_v3_hex(&self, graph_hash_hex: &str, graph_size: u16, percentage_x10: u16, timeout_ms: u64) -> Vec<u16> {
        let mut path: Vec<u16> = Vec::with_capacity(graph_size as usize);
        let mut visited = vec![false; graph_size as usize];
        let seed = self.extract_seed_from_hash_hex(graph_hash_hex);
        
        let edges = self.generate_graph_v3_from_seed(seed, graph_size, percentage_x10);

        let start_node: u16 = 0;
        let start_time = Instant::now();

        fn dfs(
            current: u16,
            visited: &mut [bool],
            path: &mut Vec<u16>,
            edges: &Vec<Vec<bool>>,
            start_time: Instant,
            timeout_ms: u64,
            graph_size: u16,
        ) -> bool {
            if start_time.elapsed() > Duration::from_millis(timeout_ms) {
                return false;
            }

            path.push(current);
            visited[current as usize] = true;

            if path.len() == graph_size as usize && edges[current as usize][0] {
                return true;
            }

            for next in 0..graph_size as usize {
                if edges[current as usize][next] && !visited[next] {
                    if dfs(next as u16, visited, path, edges, start_time, timeout_ms, graph_size) {
                        return true;
                    }
                }
            }

            visited[current as usize] = false;
            path.pop();
            false
        }

        if dfs(start_node, &mut visited, &mut path, &edges, start_time, timeout_ms, graph_size) {
            return path;
        }

        Vec::new()
    }
}
