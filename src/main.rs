use indicatif::ParallelProgressIterator;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use serde::Deserialize;
use std::path::PathBuf;
use std::{fs, process};

// todo:
// - Dry run?
// - What to do with privacy files?

#[derive(Debug, Deserialize)]
struct Options {
    privacy_zones: Vec<PrivacyZone>,
    directory: String,
}

#[derive(Debug, Deserialize)]
struct PrivacyZone {
    name: String,
    centre: Location,
    distance: f64,
}

#[derive(Debug, Deserialize)]
struct Location {
    latitude: f64,
    longitude: f64,
}

impl From<rexiv2::GpsInfo> for Location {
    fn from(g: rexiv2::GpsInfo) -> Self {
        Location { latitude: g.latitude, longitude: g.longitude }
    }
}

fn main() {
    let options: Options =
        serde_yaml::from_str(&fs::read_to_string("config.yml").unwrap()).unwrap();

    let paths: Vec<PathBuf> = match fs::read_dir(&options.directory) {
        Ok(paths) => paths.map(|f| f.unwrap().path()).collect(),
        Err(_) => {
            eprintln!("Could not open directory {}", &options.directory);
            process::exit(1);
        }
    };

    rexiv2::set_log_level(rexiv2::LogLevel::ERROR);

    let progress = ProgressBar::new(paths.len() as u64);
    progress.set_style(ProgressStyle::default_bar().template(
        "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} (ETA: {eta})",
    ));

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

    if !options.privacy_zones.is_empty() {
        let image_gps_info = meta.get_gps_info().ok_or_else(|| {
            format!(
                "{}: Image is missing GPSLatitude and/or GPSLongitude",
                path_str
            )
        })?;

        let image_location = image_gps_info.into();

        for privacy_zone in &options.privacy_zones {
            let distance = haversine_distance(&image_location, &privacy_zone.centre);
            if distance <= privacy_zone.distance {
                return Err(format!(
                    "{}: Image is inside privacy zone \"{}\"",
                    path_str, privacy_zone.name
                ));
            }
        }
    }

    meta.save_to_file(&path)
        .map_err(|_| format!("{}: Failed to save file", path_str))?;

    Ok(())
}

fn haversine_distance(start: &Location, end: &Location) -> f64 {
    let haversine_fn = |theta: f64| (1.0 - theta.cos()) / 2.0;

    let phi1 = start.latitude.to_radians();
    let phi2 = end.latitude.to_radians();
    let lambda1 = start.longitude.to_radians();
    let lambda2 = end.longitude.to_radians();

    let hav_delta_phi = haversine_fn(phi2 - phi1);
    let hav_delta_lambda = phi1.cos() * phi2.cos() * haversine_fn(lambda2 - lambda1);
    let total_delta = hav_delta_phi + hav_delta_lambda;

    (2.0 * 6371e3 * total_delta.sqrt().asin() * 1000.0).round() / 1000.0
}
