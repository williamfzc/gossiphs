use scip::types::Index;
use serde::Serialize;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use protobuf::Message;
use std::collections::{HashMap, HashSet};

#[derive(Serialize, Debug, PartialEq, Eq, Hash, Clone)]
struct UnifiedSymbol {
    file: String,
    name: String,
    line: i32,
    col: i32,
}

#[derive(Serialize, Debug, PartialEq, Eq, Hash, Clone)]
struct UnifiedRelation {
    src_file: String,
    dst_file: String,
    symbol_name: String,
}

#[derive(Serialize, Debug, PartialEq, Eq, Hash, Clone)]
struct UnifiedFileLink {
    src_file: String,
    dst_file: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: aligner <scip_file> <gossiphs_index>");
        std::process::exit(1);
    }

    let scip_path = &args[1];
    let gossiphs_path = &args[2];

    // --- 1. SCIP Extraction ---
    let mut scip_file = File::open(scip_path)?;
    let mut buffer = Vec::new();
    scip_file.read_to_end(&mut buffer)?;
    let index = Index::parse_from_bytes(&buffer[..])?;

    let mut scip_symbols = HashSet::new();
    let mut scip_relations = HashSet::new();
    let mut scip_file_links = HashSet::new();
    
    // Build symbol to file mapping from definitions
    let mut symbol_to_def_file = HashMap::new();
    for doc in &index.documents {
        for occ in &doc.occurrences {
            if (occ.symbol_roles & 1) != 0 { // Is definition
                symbol_to_def_file.insert(occ.symbol.clone(), doc.relative_path.clone());
            }
        }
    }

    for doc in &index.documents {
        for occ in &doc.occurrences {
            if occ.symbol.is_empty() || occ.symbol.starts_with("local ") { continue; }
            
            scip_symbols.insert(UnifiedSymbol {
                file: doc.relative_path.clone(),
                name: occ.symbol.clone(),
                line: occ.range[0],
                col: occ.range[1],
            });

            // If it's a reference, try to find the definition file
            if (occ.symbol_roles & 1) == 0 {
                if let Some(def_file) = symbol_to_def_file.get(&occ.symbol) {
                    if def_file != &doc.relative_path {
                        scip_relations.insert(UnifiedRelation {
                            src_file: doc.relative_path.clone(),
                            dst_file: def_file.clone(),
                            symbol_name: occ.symbol.clone(),
                        });
                        scip_file_links.insert(UnifiedFileLink {
                            src_file: doc.relative_path.clone(),
                            dst_file: def_file.clone(),
                        });
                    }
                }
            }
        }
    }

    // --- 2. gossiphs Extraction ---
    let gossiphs_file = File::open(gossiphs_path)?;
    let reader = BufReader::new(gossiphs_file);
    let mut gossiphs_symbols = HashSet::new();
    let mut gossiphs_relations = HashSet::new();
    let mut gossiphs_file_links = HashSet::new();
    
    let mut id_to_file = HashMap::new();
    let mut id_to_symbol_node = HashMap::new();
    let mut lines = Vec::new();

    for line in reader.lines() {
        let l = line?;
        let v: serde_json::Value = serde_json::from_str(&l)?;
        if v["kind"] == "FileNode" {
            id_to_file.insert(v["id"].as_u64().unwrap(), v["name"].as_str().unwrap().to_string());
        } else if v["kind"] == "SymbolNode" {
            let id = v["id"].as_u64().unwrap();
            let name = v["name"].as_str().unwrap().to_string();
            id_to_symbol_node.insert(id, name.clone());
        }
        lines.push(v);
    }

    for v in &lines {
        if v["kind"] == "FileRelation" {
            let src_file = id_to_file.get(&v["src"].as_u64().unwrap()).cloned().unwrap_or_default();
            let dst_file = id_to_file.get(&v["dst"].as_u64().unwrap()).cloned().unwrap_or_default();
            
            // Always capture the file link
            gossiphs_file_links.insert(UnifiedFileLink {
                src_file: src_file.clone(),
                dst_file: dst_file.clone(),
            });

            let symbol_ids = v["symbols"].as_array().unwrap();
            for s_id_val in symbol_ids {
                let s_id = s_id_val.as_u64().unwrap();
                if let Some(s_name) = id_to_symbol_node.get(&s_id) {
                    gossiphs_relations.insert(UnifiedRelation {
                        src_file: src_file.clone(),
                        dst_file: dst_file.clone(),
                        symbol_name: s_name.clone(),
                    });
                    
                    gossiphs_symbols.insert(UnifiedSymbol {
                        file: src_file.clone(),
                        name: s_name.clone(),
                        line: -1,
                        col: -1,
                    });
                }
            }
        }
    }

    // --- 3. Save ---
    let output = serde_json::json!({
        "scip": {
            "symbols": scip_symbols.into_iter().collect::<Vec<_>>(),
            "relations": scip_relations.into_iter().collect::<Vec<_>>(),
            "file_links": scip_file_links.into_iter().collect::<Vec<_>>(),
        },
        "gossiphs": {
            "symbols": gossiphs_symbols.into_iter().collect::<Vec<_>>(),
            "relations": gossiphs_relations.into_iter().collect::<Vec<_>>(),
            "file_links": gossiphs_file_links.into_iter().collect::<Vec<_>>(),
        }
    });

    let mut f = File::create("eval/aligned_data.json")?;
    f.write_all(serde_json::to_string_pretty(&output)?.as_bytes())?;

    println!("Alignment complete. Data saved to eval/aligned_data.json");
    Ok(())
}
