use indicatif::ParallelProgressIterator;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use serde::Deserialize;
use std::path::PathBuf;
use std::{fs, process};
extern crate custom_error;
use custom_error::custom_error;

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

custom_error! {ProcessError
    ExifFromPath{path: String}          = "Could not read metadata",
    MissingDateStamp{path: String}      = "Image is missing GPSDateStamp value",
    MissingTimeStamp{path: String}      = "Image is missing GPSTimeStamp value",
    SaveDateTime{path: String}          = "Failed to save DateTimeOriginal value",
    SaveFile{path: String}              = "Failed to save file",
    Privacy{zone: String, path: String} = "Image is inside privacy zone {zone}",
    MissingCoordinates{path: String}    = "Image is missing GPSLatitude and/or GPSLongitude",
}

impl ProcessError {
    fn path(&self) -> String {
        match &self {
            ProcessError::ExifFromPath { path }
            | ProcessError::MissingDateStamp { path }
            | ProcessError::MissingTimeStamp { path }
            | ProcessError::SaveDateTime { path }
            | ProcessError::SaveFile { path }
            | ProcessError::Privacy { path, zone: _ }
            | ProcessError::MissingCoordinates { path } => path.to_string(),
        }
    }
}

impl From<rexiv2::GpsInfo> for Location {
    fn from(g: rexiv2::GpsInfo) -> Self {
        Location {
            latitude: g.latitude,
            longitude: g.longitude,
        }
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

    let errors: Vec<ProcessError> = paths
        .par_iter()
        .progress_with(progress)
        .map(|p| process_file(&p, &options))
        .filter_map(|r| r.err())
        .collect();

    for error in &errors {
        eprintln!("Failed to process {}: {}", error.path(), error.to_string(),);
    }

    println!("Processed {} files", paths.len() - errors.len());
    println!("Failed to process {} files", errors.len());
}

fn process_file(path: &PathBuf, options: &Options) -> Result<(), ProcessError> {
    let path_str = path.to_str().unwrap();

    let meta = rexiv2::Metadata::new_from_path(&path).map_err(|_| ProcessError::ExifFromPath {
        path: path_str.to_string(),
    })?;

    let date = meta
        .get_tag_string("Exif.GPSInfo.GPSDateStamp")
        .map_err(|_| ProcessError::MissingDateStamp {
            path: path_str.to_string(),
        })?;

    let time = meta
        .get_tag_interpreted_string("Exif.GPSInfo.GPSTimeStamp")
        .map_err(|_| ProcessError::MissingTimeStamp {
            path: path_str.to_string(),
        })?;

    // Source for capacity: YY:MM:DD HH:MM:SS
    let mut date_time = String::with_capacity(19);
    date_time.push_str(&date);
    date_time.push(' ');
    date_time.push_str(&time);

    meta.set_tag_string("Exif.Photo.DateTimeOriginal", &date_time)
        .map_err(|_| ProcessError::SaveDateTime {
            path: path_str.to_string(),
        })?;

    if !options.privacy_zones.is_empty() {
        let image_gps_info =
            meta.get_gps_info()
                .ok_or_else(|| ProcessError::MissingCoordinates {
                    path: path_str.to_string(),
                })?;

        let image_location = image_gps_info.into();

        for privacy_zone in &options.privacy_zones {
            let distance = haversine_distance(&image_location, &privacy_zone.centre);
            if distance <= privacy_zone.distance {
                return Err(ProcessError::Privacy {
                    path: path_str.to_string(),
                    zone: privacy_zone.name.to_string(),
                });
            }
        }
    }

    meta.save_to_file(&path)
        .map_err(|_| ProcessError::SaveFile {
            path: path_str.to_string(),
        })?;

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
