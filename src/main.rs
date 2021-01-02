use indicatif::ParallelProgressIterator;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::fs;
use std::path::PathBuf;

fn main() {
    let paths: Vec<PathBuf> = fs::read_dir("data")
        .unwrap()
        .map(|f| f.unwrap().path())
        .collect();

    rexiv2::set_log_level(rexiv2::LogLevel::ERROR);

    let progress = ProgressBar::new(paths.len() as u64);
    progress.set_style(ProgressStyle::default_bar().template(
        "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} (ETA: {eta})",
    ));

    let errors: Vec<String> = paths
        .par_iter()
        .progress_with(progress)
        .map(|p| process_file(&p))
        .filter_map(|r| r.err())
        .collect();

    for error in &errors {
        println!("Failed to process {}", error.to_string());
    }

    println!("Processed {} files", paths.len() - errors.len());
    println!("Failed to process {} files", errors.len());
}

fn process_file(path: &PathBuf) -> Result<(), String> {
    let meta = rexiv2::Metadata::new_from_path(&path)
        .map_err(|_e| format!("{}: Could not read metadata", path.to_str().unwrap()))?;

    let date = meta
        .get_tag_string("Exif.GPSInfo.GPSDateStamp")
        .map_err(|_e| {
            format!(
                "{}: Image is missing GPSDateStamp value",
                path.to_str().unwrap()
            )
        })?;

    let time = meta
        .get_tag_interpreted_string("Exif.GPSInfo.GPSTimeStamp")
        .map_err(|_e| {
            format!(
                "{}: Image is missing GPSTimeStamp value",
                path.to_str().unwrap()
            )
        })?;

    // Source for capacity: YY:MM:DD HH:MM:SS
    let mut date_time = String::with_capacity(19);
    date_time.push_str(&date);
    date_time.push(' ');
    date_time.push_str(&time);

    meta.set_tag_string("Exif.Photo.DateTimeOriginal", &date_time)
        .map_err(|_e| {
            format!(
                "{}: Failed to save DateTimeOriginal value",
                path.to_str().unwrap()
            )
        })?;

    meta.save_to_file(&path)
        .map_err(|_e| format!("{}: Failed to save file", path.to_str().unwrap()))?;

    Ok(())
}
