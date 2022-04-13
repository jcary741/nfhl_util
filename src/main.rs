#![cfg_attr(debug_assertions, allow(dead_code, unused_imports))]

use std::collections::HashMap;
use std::ffi::OsString;
use std::fs::File;
use std::path::PathBuf;
use std::process::exit;

use clap::{Args, Parser, Subcommand};
use serde_json::{json};
use serde::{Serialize, Deserialize};
use regex::Regex;

use scraper::{Html, Selector};

#[derive(Debug, Parser)]
#[clap(name = "nfhl_util")]
#[clap(author, version, about = "A tool to inventory FEMA FIRM/NFHL files and layers.", long_about = None)]
struct Cli {
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Lists effective NFHL file urls for all states, keyed by 2-digit fips codes.
    #[clap(name = "states_inventory", arg_required_else_help = true)]
    States {
        /// Where to save the inventory JSON file.
        #[clap(long, parse(from_os_str))]
        outfile: PathBuf,
        /// A coefficient used to spread out queries to FEMA's servers. Higher number = fewer threads / longer delay between queries.
        #[clap(long, default_value_t = u8::MAX)]
        politeness: u8,
    },
    /// Lists effective NFHL file urls for all counties, keyed by 5-digit fips codes.
    #[clap(name = "counties_inventory", arg_required_else_help = true)]
    Counties {
        /// Where to save the inventory JSON file.
        #[clap(long, parse(from_os_str))]
        outfile: PathBuf,
        /// A coefficient used to spread out requests to FEMA's servers. Higher number = fewer threads / longer delay between requests.
        #[clap(long, default_value_t = u8::MAX)]
        politeness: u8,
    },
    /// Downloads effective NFHL file urls for all counties, keyed by 5-digit fips codes.
    #[clap(name = "download_all", arg_required_else_help = true)]
    DownloadAll {
        /// The current inventory JSON file.
        inventory: String,
        /// Where to cache files.
        #[clap(long, parse(from_os_str))]
        cache_dir: PathBuf,
        /// A previous inventory JSON file. Entries which have changed will be re-downloaded, even if the file was already in the cache.
        #[clap(long, parse(from_os_str))]
        old_inventory: Option<PathBuf>,
        /// Whether to delete files from the cache directory which are no longer in the inventory.
        #[clap(long)]
        delete: bool,
        /// A coefficient used to spread out requests to FEMA's servers. Higher number = fewer threads / longer delay between requests.
        #[clap(long, default_value_t = u8::MAX)]
        politeness: u8,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Cli::parse();

    match args.command {
        Commands::States { outfile, politeness } => {
            if let Some(outfile_dir) = outfile.parent() {
                std::fs::create_dir_all(outfile_dir)?;
            }
            // let mut f = File::create(outfile)?;
            //
            // inventory_states();
            //
            //
            // let json = json!({
            //     "name": "John Doe",
            //     "age": 43,
            //     "phones": [
            //         "+44 1234567",
            //         "+44 2345678"
            //     ]
            // });
            // serde_json::to_writer(f, &json)?;
        }
        Commands::Counties { outfile, politeness } => {
            if let Some(outfile_dir) = outfile.parent() {
                std::fs::create_dir_all(outfile_dir)?;
            }
            let f = File::create(outfile)?;

            let inv = get_all_effective_county_products().unwrap();

            serde_json::to_writer(f, &inv)?;
        }
        Commands::DownloadAll { inventory, cache_dir, old_inventory, delete, politeness } => {}
    }
    Ok(())
}

#[derive(Serialize, Deserialize, Debug)]
pub struct InventoryEntry {
    effective_file_url: String,
    effective_file_date: String,
    preliminary_file_url: String,
    preliminary_file_date: String,
}

pub fn inventory_states() -> Result<HashMap<String, InventoryEntry>, Box<dyn std::error::Error>> {
    // let fema_region_states = vec![
    //     Vec!["ME", "NH", "VT", "MA", "CT", "RI"],
    //     Vec!["NY", "NJ", "PR", "VI"],
    //     Vec!["MD", "PA", "WV", "DC", "DE", "VA"],
    //     Vec!["NC", "SC", "GA", "FL", "AL", "MS", "TN", "KY"],
    //     Vec!["IL", "IN", "OH", "MI", "WI", "MN"],
    //     Vec!["NM", "TX", "OK", "LA", "AR"],
    //     Vec!["NE", "IA", "KS", "MO"],
    //     Vec!["MT", "ND", "SD", "WY", "UT", "CO"],
    //     Vec!["NV", "AZ", "CA", "FM", "GU", "HI", "MH", "MP", "AS"],
    //     Vec!["AK", "WA", "OR", "ID"],
    // ];

    let state_to_fips = HashMap::from([
        ("AK", "02"),
        ("AL", "01"),
        ("AR", "05"),
        ("AS", "60"),
        ("AZ", "04"),
        ("CA", "06"),
        ("CO", "08"),
        ("CT", "09"),
        ("DC", "11"),
        ("DE", "10"),
        ("FL", "12"),
        ("GA", "13"),
        ("GU", "66"),
        ("HI", "15"),
        ("IA", "19"),
        ("ID", "16"),
        ("IL", "17"),
        ("IN", "18"),
        ("KS", "20"),
        ("KY", "21"),
        ("LA", "22"),
        ("MA", "25"),
        ("MD", "24"),
        ("ME", "23"),
        ("MI", "26"),
        ("MN", "27"),
        ("MO", "29"),
        ("MS", "28"),
        ("MT", "30"),
        ("NC", "37"),
        ("ND", "38"),
        ("NE", "31"),
        ("NH", "33"),
        ("NJ", "34"),
        ("NM", "35"),
        ("NV", "32"),
        ("NY", "36"),
        ("OH", "39"),
        ("OK", "40"),
        ("OR", "41"),
        ("PA", "42"),
        ("PR", "72"),
        ("RI", "44"),
        ("SC", "45"),
        ("SD", "46"),
        ("TN", "47"),
        ("TX", "48"),
        ("UT", "49"),
        ("VA", "51"),
        ("VI", "78"),
        ("VT", "50"),
        ("WA", "53"),
        ("WI", "55"),
        ("WV", "54"),
        ("WY", "56"),
        ("MH", "68"), // may not be available in MSC
        ("MP", "69"),
        ("FM", "64") // may not be available in MSC
    ]);

    let inv = HashMap::<String, InventoryEntry>::with_capacity(57);
    for (state, state_code) in state_to_fips.iter() {
        let state_code = state_to_fips[state];
        // https://msc.fema.gov/portal/advanceSearch
        let client = reqwest::blocking::Client::builder().cookie_store(true).build()?;
        client.post("https://www.lycamobile.es/wp-admin/admin-ajax.php")
            .form(&[
                ("action", "lyca_login_ajax"),
                ("method", "login"),
                ("mobile_no", "<MOBILE_PHONE_NUMBER>"),
                ("pass", "<SUPER_SECRET_PASSWORD>")
            ])
            .send()?;

        let response = client.get("https://www.lycamobile.es/es/my-account/").send()?;
        let body_response = response.text()?;
    }

    Ok(inv)
}


pub fn get_all_effective_county_products() -> Result<HashMap<String, InventoryEntry>, Box<dyn std::error::Error>> {
    let client = reqwest::blocking::Client::builder().cookie_store(true).build()?;
    // client.post("https://www.lycamobile.es/wp-admin/admin-ajax.php")
    //     .form(&[
    //         ("action", "lyca_login_ajax"),
    //         ("method", "login"),
    //         ("mobile_no", "<MOBILE_PHONE_NUMBER>"),
    //         ("pass", "<SUPER_SECRET_PASSWORD>")
    //     ])
    //     .send()?;

    let response = client.get("https://hazards.fema.gov/femaportal/NFHL/searchResult").send()?;
    let body_response = response.text()?;
    let parsed_html = Html::parse_document(&body_response);
    let tr_selector = &Selector::parse("tbody tr").expect("selector parse error");
    let a_selector = Selector::parse("a").unwrap();

    let re = Regex::new(r"fileName=(.+?)[cC]_(.+?).zip").unwrap();
    let mut inv = HashMap::<String, InventoryEntry>::with_capacity(57);
    for tr in parsed_html.select(&tr_selector) {
        if let Some(a) = tr.select(&a_selector).next() {
            let file_url = a.value().attr("href").unwrap();
            if let Some(caps) = re.captures(file_url) {
                let county_fips = caps.get(1).map_or("", |m| m.as_str());
                let date = caps.get(2).map_or("", |m| m.as_str());
                inv.insert(county_fips.to_string(), InventoryEntry {
                    effective_file_url: "https://hazards.fema.gov/femaportal/NFHL/".to_string() + file_url,
                    effective_file_date: date.to_string(),
                    preliminary_file_url: "".to_string(),
                    preliminary_file_date: "".to_string(),
                });
            }
        }
    }
    Ok(inv)
}
