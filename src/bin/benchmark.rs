use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use khoj::model::Model;
use khoj::add_folder_to_model;

fn main() {
    println!("Starting benchmarks...");

    // 1. Setup paths
    let current_dir = std::env::current_dir().expect("Failed to get current directory");
    let target_dir = current_dir.join("annotatedCentralActs");
    
    if !target_dir.exists() {
        eprintln!("Error: Directory {:?} not found. Please run this from the project root.", target_dir);
        return;
    }

    // 2. Indexing Benchmark
    println!("\n=== Indexing Benchmark ===");
    let model = Arc::new(Mutex::new(Model::default()));
    let start_time = Instant::now();
    let mut processed_files = 0;
    
    match add_folder_to_model(&target_dir, Arc::clone(&model), &mut processed_files) {
        Ok(_) => {
            let duration = start_time.elapsed();
            println!("Indexed {} files in {:.2?}", processed_files, duration);
            if processed_files > 0 {
                // Approximate files per second
                let fps = processed_files as f64 / duration.as_secs_f64();
                println!("Indexing Throughput: {:.2} files/sec", fps);
            }
        },
        Err(_) => {
            eprintln!("Failed to index directory.");
            return;
        }
    }

    // 3. Search Benchmark
    println!("\n=== Search Benchmark ===");
    let search_terms = vec![
        "act", "section", "government", "penalty", "offence", 
        "rule", "order", "court", "judge", "police"
    ];

    let model_guard = model.lock().unwrap();
    let warmup_queries = 10;
    
    // Warmup
    for _ in 0..warmup_queries {
        for term in &search_terms {
           let query_chars: Vec<char> = term.chars().collect();
           let _ = model_guard.search_query(&query_chars);
        }
    }

    // Latency Test
    let mut total_latency = std::time::Duration::new(0, 0);
    let mut query_count = 0;
    
    let iterations = 100;
    for _ in 0..iterations {
        for term in &search_terms {
            let query_chars: Vec<char> = term.chars().collect();
            let start = Instant::now();
            let results = model_guard.search_query(&query_chars);
            total_latency += start.elapsed();
            query_count += 1;
            
            // Sanity check to ensure we are actually searching
            if results.is_empty() && iterations == 0 {
                 // println!("Warning: No results found for '{}'", term);
            }
        }
    }

    let avg_latency = total_latency / query_count as u32;
    println!("Average Search Latency: {:.2?}", avg_latency);

    // Throughput Test
    println!("\n=== Search Throughput Benchmark (5s) ===");
    let throughput_duration = std::time::Duration::from_secs(5);
    let start_throughput = Instant::now();
    let mut total_queries = 0;
    
    while start_throughput.elapsed() < throughput_duration {
        for term in &search_terms {
            let query_chars: Vec<char> = term.chars().collect();
            let _ = model_guard.search_query(&query_chars);
            total_queries += 1;
        }
    }
    
    let actual_duration = start_throughput.elapsed();
    let qps = total_queries as f64 / actual_duration.as_secs_f64();
    println!("Total Queries: {}", total_queries);
    println!("Throughput: {:.2} QPS", qps);
}
