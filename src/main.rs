use indicatif::ParallelProgressIterator;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use serde::Deserialize;
use std::fs::{create_dir_all, rename};
use std::path::{Path, PathBuf};
use std::{fs, process};

// todo:
// - Dry run?

#[derive(Debug, Deserialize)]
struct Options {
    #[serde(default)]
    privacy_zones: Vec<PrivacyZone>,
    #[serde(default)]
    use_gps_timestamps: bool,
    input_directory: String,
    failed_directory: String,
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

#[derive(Debug)]
enum ProcessError<'a> {
    ExifFromPath { path: &'a Path },
    MissingDateStamp { path: &'a Path },
    MissingTimeStamp { path: &'a Path },
    SaveDateTime { path: &'a Path },
    SaveFile { path: &'a Path },
    Privacy { zone: String, path: &'a Path },
    MissingCoordinates { path: &'a Path },
}

impl std::error::Error for ProcessError<'_> {}

impl std::fmt::Display for ProcessError<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ProcessError::ExifFromPath { path: _ } => {
                write!(f, "Could not read metadata")
            }
            ProcessError::MissingDateStamp { path: _ } => {
                write!(f, "Image is missing GPSDateStamp value")
            }
            ProcessError::MissingTimeStamp { path: _ } => {
                write!(f, "Image is missing GPSTimeStamp value")
            }
            ProcessError::SaveDateTime { path: _ } => {
                write!(f, "Failed to save DateTimeOriginal value")
            }
            ProcessError::SaveFile { path: _ } => {
                write!(f, "Failed to save file")
            }
            ProcessError::Privacy { path: _, zone } => {
                write!(f, "Image is inside privacy zone {}", zone)
            }
            ProcessError::MissingCoordinates { path: _ } => {
                write!(f, "Image is missing GPSLatitude and/or GPSLongitude")
            }
        }
    }
}

impl ProcessError<'_> {
    fn path(&self) -> &Path {
        match &self {
            ProcessError::ExifFromPath { path }
            | ProcessError::MissingDateStamp { path }
            | ProcessError::MissingTimeStamp { path }
            | ProcessError::SaveDateTime { path }
            | ProcessError::SaveFile { path }
            | ProcessError::Privacy { path, zone: _ }
            | ProcessError::MissingCoordinates { path } => path,
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

    if !options.use_gps_timestamps && options.privacy_zones.is_empty() {
        eprintln!("Nothing to do!");
        process::exit(1);
    }

    create_dir_all(&options.failed_directory).unwrap();

    let paths: Vec<PathBuf> = match fs::read_dir(&options.input_directory) {
        Ok(paths) => paths.map(|f| f.unwrap().path()).collect(),
        Err(_) => {
            eprintln!("Could not open directory {}", &options.input_directory);
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
        let new_file_name = error.path().file_name().unwrap().to_str().unwrap();
        let mut new_path =
            String::with_capacity(&options.failed_directory.len() + new_file_name.len() + 1);
        new_path.push_str(&options.failed_directory);
        new_path.push('/');
        new_path.push_str(new_file_name);

        eprintln!(
            "Failed to process {}: {} -> {}",
            error.path().to_str().unwrap(),
            error.to_string(),
            new_path,
        );

        rename(error.path(), new_path).unwrap();
    }

    println!("Processed {} files", paths.len() - errors.len());
    println!("Failed to process {} files", errors.len());
}

fn process_file<'a>(path: &'a PathBuf, options: &Options) -> Result<(), ProcessError<'a>> {
    let meta = rexiv2::Metadata::new_from_path(&path).map_err(|_| ProcessError::ExifFromPath {
        path: path.as_path(),
    })?;

    if options.use_gps_timestamps {
        let date = meta
            .get_tag_string("Exif.GPSInfo.GPSDateStamp")
            .map_err(|_| ProcessError::MissingDateStamp {
                path: path.as_path(),
            })?;

        let time = meta
            .get_tag_interpreted_string("Exif.GPSInfo.GPSTimeStamp")
            .map_err(|_| ProcessError::MissingTimeStamp {
                path: path.as_path(),
            })?;

        // Source for capacity: YY:MM:DD HH:MM:SS
        let mut date_time = String::with_capacity(19);
        date_time.push_str(&date);
        date_time.push(' ');
        date_time.push_str(&time);

        meta.set_tag_string("Exif.Photo.DateTimeOriginal", &date_time)
            .map_err(|_| ProcessError::SaveDateTime {
                path: path.as_path(),
            })?;
    }

    if !options.privacy_zones.is_empty() {
        let image_gps_info =
            meta.get_gps_info()
                .ok_or_else(|| ProcessError::MissingCoordinates {
                    path: path.as_path(),
                })?;

        let image_location = image_gps_info.into();

        for privacy_zone in &options.privacy_zones {
            let distance = haversine_distance(&image_location, &privacy_zone.centre);
            if distance <= privacy_zone.distance {
                return Err(ProcessError::Privacy {
                    path: path.as_path(),
                    zone: privacy_zone.name.to_string(),
                });
            }
        }
    }

    meta.save_to_file(&path)
        .map_err(|_| ProcessError::SaveFile {
            path: path.as_path(),
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
