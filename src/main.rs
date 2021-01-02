use indicatif::ParallelProgressIterator;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::path::PathBuf;
use std::{env, fs, process};
use geoutils::{Location, Distance};

struct Options {
    privacy_zones: Vec<PrivacyZone>
}

struct PrivacyZone {
    name: &'static str,
    centre: Location,
    distance: Distance,
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let dir = match args.get(1) {
        Some(dir) => dir,
        None => {
            eprintln!("Directory is required");
            process::exit(1);
        }
    };

    let paths: Vec<PathBuf> = match fs::read_dir(dir) {
        Ok(paths) => paths.map(|f| f.unwrap().path()).collect(),
        Err(_) => {
            eprintln!("Could not open directory {}", dir);
            process::exit(1);
        }
    };

    rexiv2::set_log_level(rexiv2::LogLevel::ERROR);

    let progress = ProgressBar::new(paths.len() as u64);
    progress.set_style(ProgressStyle::default_bar().template(
        "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} (ETA: {eta})",
    ));

    let options = Options {
        privacy_zones: vec![

        ],
    };

    let errors: Vec<String> = paths
        .par_iter()
        .progress_with(progress)
        .map(|p| process_file(&p, &options))
        .filter_map(|r| r.err())
        .collect();

    for error in &errors {
        eprintln!("Failed to process {}", error.to_string());
    }

    println!("Processed {} files", paths.len() - errors.len());
    println!("Failed to process {} files", errors.len());
}

fn process_file(path: &PathBuf, options: &Options) -> Result<(), String> {
    let path_str = path.to_str().unwrap();

    let meta = rexiv2::Metadata::new_from_path(&path)
        .map_err(|_| format!("{}: Could not read metadata", path_str))?;

    let date = meta
        .get_tag_string("Exif.GPSInfo.GPSDateStamp")
        .map_err(|_| format!("{}: Image is missing GPSDateStamp value", path_str))?;

    let time = meta
        .get_tag_interpreted_string("Exif.GPSInfo.GPSTimeStamp")
        .map_err(|_| format!("{}: Image is missing GPSTimeStamp value", path_str))?;

    // Source for capacity: YY:MM:DD HH:MM:SS
    let mut date_time = String::with_capacity(19);
    date_time.push_str(&date);
    date_time.push(' ');
    date_time.push_str(&time);

    meta.set_tag_string("Exif.Photo.DateTimeOriginal", &date_time)
        .map_err(|_| format!("{}: Failed to save DateTimeOriginal value", path_str))?;

    if ! options.privacy_zones.is_empty() {
        let image_gps_info = meta.get_gps_info()
            .ok_or_else(|| format!("{}: Image is missing GPSLatitude and/or GPSLongitude", path_str))?;
        let image_location = Location::new(image_gps_info.latitude, image_gps_info.longitude);

        for privacy_zone in &options.privacy_zones {
            if image_location.haversine_distance_to(&privacy_zone.centre).meters() <= privacy_zone.distance.meters() {
                return Err(format!("{}: Image is inside privacy zone {}", path_str, privacy_zone.name));
            }
        }
    }

    meta.save_to_file(&path)
        .map_err(|_| format!("{}: Failed to save file", path_str))?;

    Ok(())
}
