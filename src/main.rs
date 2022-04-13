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
            let f = File::create(outfile)?;

            let inv = get_effective_state_products().unwrap();

            serde_json::to_writer(f, &inv)?;
        }
        Commands::Counties { outfile, politeness } => {
            if let Some(outfile_dir) = outfile.parent() {
                std::fs::create_dir_all(outfile_dir)?;
            }
            let f = File::create(outfile)?;

            let inv = get_effective_county_products().unwrap();

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

#[derive(Deserialize, Debug)]
pub struct SearchResults {
    #[serde(rename(deserialize = "EFFECTIVE"))]
    effective: SearchResultEffective,
    #[serde(rename(deserialize = "PRELIM_FIRM_DB"))]
    preliminary: Option<Vec<SearchResultProductEntry>>,
}

#[derive(Deserialize, Debug)]
pub struct SearchResultEffective {
    #[serde(rename(deserialize = "NFHL_COUNTY_DATA"))]
    county: Option<Vec<SearchResultProductEntry>>,
    #[serde(rename(deserialize = "NFHL_STATE_DATA"))]
    state: Option<Vec<SearchResultProductEntry>>,
}

#[derive(Deserialize, Debug)]
pub struct SearchResultProductEntry {
    #[serde(rename(deserialize = "product_TYPE_ID"))]
    type_id: String,
    #[serde(rename(deserialize = "product_SUBTYPE_ID"))]
    subtype_id: String,
    #[serde(rename(deserialize = "product_NAME"))]
    name: String,
    #[serde(rename(deserialize = "product_ID"))]
    id: usize, // so far as I can tell, these ids are useless. Use "name" instead.
    #[serde(rename(deserialize = "product_EFFECTIVE_DATE_STRING"))]
    effective_date: Option<String>,
    #[serde(rename(deserialize = "product_FILE_PATH"))]
    filename: Option<String>,
    #[serde(rename(deserialize = "product_FILE_SIZE"))]
    filesize: Option<String>
}



pub fn get_effective_state_products() -> Result<HashMap<String, InventoryEntry>, Box<dyn std::error::Error>> {
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

    // in order to query msc.fema.gov, we must look for a specific community. To that end, each state has a county.
    let state_to_representative_county = HashMap::from([
        // ("AK", "02"),
        ("AL", "01101"),
        ("AR", "05029"),
        // ("AS", "60"),
        // ("AZ", "04"),
        // ("CA", "06"),
        // ("CO", "08"),
        // ("CT", "09"),
        // ("DC", "11"),
        // ("DE", "10"),
        // ("FL", "12"),
        // ("GA", "13"),
        // ("GU", "66"),
        // ("HI", "15"),
        // ("IA", "19"),
        // ("ID", "16"),
        // ("IL", "17"),
        // ("IN", "18"),
        // ("KS", "20"),
        // ("KY", "21"),
        // ("LA", "22"),
        // ("MA", "25"),
        // ("MD", "24"),
        // ("ME", "23"),
        // ("MI", "26"),
        // ("MN", "27"),
        // ("MO", "29"),
        // ("MS", "28"),
        // ("MT", "30"),
        // ("NC", "37"),
        // ("ND", "38"),
        // ("NE", "31"),
        // ("NH", "33"),
        // ("NJ", "34"),
        // ("NM", "35"),
        // ("NV", "32"),
        // ("NY", "36"),
        // ("OH", "39"),
        // ("OK", "40"),
        // ("OR", "41"),
        // ("PA", "42"),
        // ("PR", "72"),
        // ("RI", "44"),
        // ("SC", "45"),
        // ("SD", "46"),
        // ("TN", "47"),
        // ("TX", "48"),
        // ("UT", "49"),
        // ("VA", "51"),
        // ("VI", "78"),
        // ("VT", "50"),
        // ("WA", "53"),
        // ("WI", "55"),
        // ("WV", "54"),
        // ("WY", "56"),
        // ("MH", "68"), // may not be available in MSC
        // ("MP", "69"),
        // ("FM", "64") // may not be available in MSC
    ]);



    let mut inv = HashMap::<String, InventoryEntry>::with_capacity(57);

    let client = reqwest::blocking::Client::builder().cookie_store(true).build()?;
    // do a search query once just to start a session (sessions are stateful)
    let a = client.get("https://msc.fema.gov/portal/advanceSearch").send()?;


    for (&state, &representative_county) in state_to_representative_county.iter(){
        let state_code = &representative_county[..2];
        // https://msc.fema.gov/portal/advanceSearch

        // let b = client.get(format!("https://msc.fema.gov/portal/advanceSearch?getCommunity={}&state={}",representative_county, state_code))
        //     .send()?;


        let b: SearchResults = client.post(format!("https://msc.fema.gov/portal/advanceSearch"))
            .form(&[
                ("utf8", "✓"), // I kid you not, this is included in every post to the official site.
                ("affiliate", "fema"),
                ("query", ""), // intentionally blank?
                ("selstate", state_code),
                ("selcounty", representative_county),
                ("selcommunity", &format!("{}C", representative_county)),
                ("jurisdictionkey", ""),
                ("searchedCid", &format!("{}C", representative_county)),
                ("searchedDateStart", ""),
                ("searchedDateEnd", ""),
                ("txtstartdate", ""),
                ("txtenddate", ""),
                ("method", "search")
            ])
            .send()?.json()?;
        dbg!(&b);
    }

    Ok(inv)
}


pub fn get_effective_county_products() -> Result<HashMap<String, InventoryEntry>, Box<dyn std::error::Error>> {
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
